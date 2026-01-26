#![no_std]

#[path = "."]
pub mod asynchronous {
    use bisync::asynchronous::*;
    use embedded_hal_async::i2c::{I2c, SevenBitAddress};
    use embedded_hal_async::spi::{SpiDevice, Operation};

    pub mod ops;
    #[cfg(feature = "i2c")]
    pub mod i2c;
    #[cfg(feature = "spi")]
    pub mod spi;

    pub use ops::*;
    #[cfg(feature = "i2c")]
    pub use i2c::*;
    #[cfg(feature = "spi")]
    pub use spi::*;
}

#[path = "."]
pub mod blocking {
    use bisync::synchronous::*;
    use embedded_hal::i2c::{I2c, SevenBitAddress};
    use embedded_hal::spi::{SpiDevice, Operation};

    pub mod ops;
    #[cfg(feature = "i2c")]
    pub mod i2c;
    #[cfg(feature = "spi")]
    pub mod spi;

    pub use ops::*;
    pub use i2c::*;
}

const CHUNK_SIZE: usize = 256;

