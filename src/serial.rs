//! Serial-line settings shared by the client and the server.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Parity {
    None,
    Even,
    Odd,
}

impl From<Parity> for tokio_serial::Parity {
    fn from(parity: Parity) -> Self {
        match parity {
            Parity::None => tokio_serial::Parity::None,
            Parity::Even => tokio_serial::Parity::Even,
            Parity::Odd => tokio_serial::Parity::Odd,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StopBits {
    #[value(name = "1")]
    One,
    #[value(name = "2")]
    Two,
}

impl From<StopBits> for tokio_serial::StopBits {
    fn from(stop_bits: StopBits) -> Self {
        match stop_bits {
            StopBits::One => tokio_serial::StopBits::One,
            StopBits::Two => tokio_serial::StopBits::Two,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DataBits {
    #[value(name = "5")]
    Five,
    #[value(name = "6")]
    Six,
    #[value(name = "7")]
    Seven,
    #[value(name = "8")]
    Eight,
}

impl From<DataBits> for tokio_serial::DataBits {
    fn from(data_bits: DataBits) -> Self {
        match data_bits {
            DataBits::Five => tokio_serial::DataBits::Five,
            DataBits::Six => tokio_serial::DataBits::Six,
            DataBits::Seven => tokio_serial::DataBits::Seven,
            DataBits::Eight => tokio_serial::DataBits::Eight,
        }
    }
}
