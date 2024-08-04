use std::sync::Arc;

use futuresdr::seify::{Args, Range};

#[derive(Debug)]
pub struct SdrLimits {
    pub freq_range: Range,
    pub gain_range: Range,
    pub sample_rate_range: Range,
}

type SdrType = futuresdr::seify::Device<
    Arc<
        dyn futuresdr::seify::DeviceTrait<
                RxStreamer = Box<(dyn futuresdr::seify::RxStreamer + 'static)>,
                TxStreamer = Box<(dyn futuresdr::seify::TxStreamer + 'static)>,
            > + Sync,
    >,
>;

pub fn new_sdr(args: &Args) -> Result<(SdrType, SdrLimits), Box<dyn std::error::Error>> {
    let device = futuresdr::seify::Device::from_args(args)?;

    let limits = SdrLimits {
        freq_range: device
            .frequency_range(futuresdr::seify::Direction::Rx, 0)
            .unwrap(),
        gain_range: device
            .gain_range(futuresdr::seify::Direction::Rx, 0)
            .unwrap(),
        sample_rate_range: device
            .get_sample_rate_range(futuresdr::seify::Direction::Rx, 0)
            .unwrap(),
    };

    Ok((device, limits))
}

#[inline]
pub fn get_devices() -> Result<Vec<Args>, futuresdr::seify::Error> {
    futuresdr::seify::enumerate()
}

pub fn get_name(args: &Args) -> String {
    let mut name = args.get::<String>("driver").unwrap();

    match name.as_str() {
        "rtlsdr" => name += &(" ".to_owned() + &args.get::<String>("index").unwrap()),
        "soapy" => {
            name += &(" ".to_owned()
                + &args.get::<String>("soapy_driver").unwrap()
                + " "
                + &args.get::<String>("label").unwrap())
        }
        _ => (),
    }

    name
}
