use syn::{
    parse::{Parse, ParseStream}, Meta, MetaList, Ident, ItemEnum, LitStr, LitInt, Result, Token, Attribute, Variant
};

use quote::quote;
use crate::generate_generics;

pub(crate) struct MemBankArgs {
    device: Ident,
    generics: Option<u8>,
}

impl Parse for MemBankArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        // Parse the first argument: an identifier
        let device: Ident = input.parse()?;

        let mut generics: Option<u8> = None;

        // If there is a comma, parse the key-value pair
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;

            // Parse the key (should be "generics")
            let key: Ident = input.parse()?;
            if key != "generics" {
                return Err(syn::Error::new_spanned(key, "Expected key `generics`"));
            }

            input.parse::<Token![=]>()?;

            let value: LitInt = input.parse()?;
            generics = Some(value.base10_parse()?);
        }

        if !input.is_empty() {
            return Err(input.error("Unexpected tokens after attribute arguments"));
        }

        Ok(MemBankArgs { device, generics })
    }
}

pub(crate) struct VariantAttr {
    pub(crate) struct_name: Ident,
    pub(crate) fn_name: LitStr
}

impl Parse for VariantAttr {
    fn parse(input: ParseStream) -> Result<Self> {

        let struct_name: Ident = input.parse()?;
        let mut fn_name = None;

        if input.peek(Token![,]) {
            let _comma: Token![,] = input.parse()?;

            let key: Ident = input.parse()?;
            if key != "fn_name" {
                return Err(syn::Error::new(key.span(), "Expected `fn_name`"));
            }
            let _eq: Token![=] = input.parse()?;
            let fn_name_lit: LitStr = input.parse()?;
            fn_name = Some(fn_name_lit);
        }

        Ok(VariantAttr { struct_name, fn_name: fn_name.unwrap() })
    }
}

pub(crate) struct MemBank {
    pub enum_name: Ident,
    pub variants: Vec<(VariantAttr, Variant)>,
    pub main_variant: Variant,
    pub enum_raw: ItemEnum,
}

impl Parse for MemBank {
    fn parse(input: ParseStream) -> Result<Self> {
        let enum_obj: ItemEnum = input.parse()?;
        let enum_name = enum_obj.ident.clone();

        let mut main = None;

        for variant in &enum_obj.variants {
            for attr in &variant.attrs {
                if attr.path().is_ident("main") {
                    main = Some(variant.clone());
                }
            }
        }

        if main.is_none() {
            panic!("No variant with is decorated with `#[main]`");
        }

        let main_variant = main.unwrap();
        let mut variants = vec![];

        for variant in &enum_obj.variants {
            for attr in &variant.attrs {
                if let Some(variant_obj) = Self::extract_state(attr) {
                    variants.push((variant_obj, variant.clone()));
                }
            }
        }

        Ok(MemBank {
            enum_name: enum_name.clone(),
            variants,
            main_variant,
            enum_raw: enum_obj
        })
    }
}

impl MemBank {

    pub fn create_output(&mut self, attr: &MemBankArgs) -> proc_macro2::TokenStream {
        let sensor_name = &attr.device;
        let result: Vec<proc_macro2::TokenStream> = self.variants.iter().map(|(variant, variant_raw)| {
            let variant_name = &variant_raw.ident;
            self.create_state_struct(&variant, &sensor_name, variant_name, attr.generics.unwrap())
        }).collect();

        for variant in &mut self.enum_raw.variants {
            variant.attrs = Self::filter_name_attrs(&variant.attrs);
        }

        let input = &self.enum_raw;

        let res = quote! {
            #input

            pub struct MainBank;

            pub trait BankState {}
            impl BankState for MainBank {}

            #(#result)*
        };

        res
    }

    fn extract_state(attr: &Attribute) -> Option<VariantAttr> {
       if attr.path().is_ident("state") {
            if let Meta::List(MetaList { tokens, .. }) = &attr.meta {
                return syn::parse2::<VariantAttr>(tokens.clone()).ok();
            }
       }
       None
    }

    fn create_state_struct(&self, variant: &VariantAttr, sensor_name: &Ident, variant_name: &Ident, generics_num: u8) -> proc_macro2::TokenStream {
        let name = &variant.struct_name;
        let fn_name = &variant.fn_name;
        let main_variant = &self.main_variant.ident;

        let generics = generate_generics(generics_num.into());
        let long_timer = generics.timer_long;
        let short_timer = generics.timer_short;


        let fn_name = Ident::new(&fn_name.value(), fn_name.span());
        let enum_name = &self.enum_name;

        quote!(
            pub struct #name;

            impl BankState for #name {}

            #[only_async]
            impl<B #short_timer, S> #sensor_name<B #short_timer, S>
            where
                B: BusOperation
                #long_timer,
                S: BankState
            {
                pub async fn #fn_name<'a, F, R>(&'a mut self, f: F) -> Result<R, Error<B::Error>>
                where
                    F: AsyncFnOnce(&'a mut #sensor_name<B #short_timer, #name>) -> Result<R, Error<B::Error>>
                {
                    // Switch membank
                    self.mem_bank_set(#enum_name :: #variant_name).await?;

                    // Transmute the state
                    // SAFETY: It's safe as long as invoked over ZST (zero sized type)
                    let extended_driver = unsafe {
                        &mut *(self as *mut Self as *mut #sensor_name<B #short_timer, #name>)
                    };

                    // Execute operations
                    let result = f(extended_driver).await;

                    // Switch back to main bank
                    self.mem_bank_set(#enum_name :: #main_variant).await?;

                    result
                }
            }

            #[only_sync]
            impl<B #short_timer, S> #sensor_name<B, T, S>
            where
                B: BusOperation
                #long_timer,
                S: BankState
            {
                pub fn #fn_name<F, R>(& mut self, f: F) -> Result<R, Error<B::Error>>
                where
                    F: FnOnce(& mut #sensor_name<B #short_timer, #name>) -> Result<R, Error<B::Error>>,
                {
                    // Switch to #variant_name
                    self.mem_bank_set(#enum_name :: #variant_name)?;

                    // Transmute the state
                    // SAFETY: It's safe as long as invoked over ZST (zero sized type)
                    let extended_driver = unsafe {
                        &mut *(self as *mut Self as *mut #sensor_name<B #short_timer, #name>)
                    };

                    // Execute operations
                    let result = f(extended_driver);

                    // Switch back to main bank
                    self.mem_bank_set(#enum_name :: #main_variant)?;

                    result
                }
            }

        )
    }

    fn filter_name_attrs(attrs: &[Attribute]) -> Vec<Attribute> {
        attrs.iter()
            .filter(|attr| {
                !attr.path().is_ident("state")
            })
            .filter(|attr| {
                !attr.path().is_ident("main")
            })
            .cloned()
            .collect()

    }

}
