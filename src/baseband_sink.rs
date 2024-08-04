use chrono::{Datelike, Timelike, Utc};
use futuresdr::anyhow::Result;
use futuresdr::runtime::Pmt;
use futuresdr::{
    anyhow::Ok,
    macros::{async_trait, message_handler},
    num_complex::Complex32,
    runtime::{
        Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIo, MessageIoBuilder, StreamIo,
        StreamIoBuilder, WorkIo,
    },
};
use hound::{self, SampleFormat, WavSpec};

#[allow(non_camel_case_types)]
#[derive(Default, Clone)]
pub enum BaseBandFormat {
    #[default]
    i16,
    f32,
    i8,
}

#[derive(Default, Clone)]
pub struct BaseBandSpec {
    pub format: BaseBandFormat,
    pub sample_rate: u32,
}

pub struct BaseBandSink {
    spec: BaseBandSpec,
    writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
}

impl BaseBandSink {
    /// Create Base Band Sink block
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("BaseBandSink").build(),
            StreamIoBuilder::new().add_input::<Complex32>("in").build(),
            MessageIoBuilder::new()
                .add_input("toggle", Self::toggle_handler)
                .add_input("spec", Self::spec_handler)
                .add_input("duration", Self::duration_handler)
                .build(),
            BaseBandSink {
                writer: None,
                spec: Default::default(),
            },
        )
    }

    #[message_handler]
    fn toggle_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if self.writer.is_some() {
            self.writer = None;
            return Ok(Pmt::Bool(false));
        } else {
            let time_stamp = Utc::now();
            let freq = match p {
                Pmt::F64(freq) => freq,
                _ => 0.0,
            };
            let file_name = format!(
                "baseband_{}Hz_{}-{}-{}_{}-{}-{}.wav",
                freq,
                time_stamp.hour(),
                time_stamp.minute(),
                time_stamp.second(),
                time_stamp.month(),
                time_stamp.day(),
                time_stamp.year()
            );
            let bit_per_sample = match self.spec.format {
                BaseBandFormat::i16 => 16,
                BaseBandFormat::f32 => 32,
                BaseBandFormat::i8 => 8,
            };
            let sample_format = match self.spec.format {
                BaseBandFormat::i16 | BaseBandFormat::i8 => SampleFormat::Int,
                BaseBandFormat::f32 => SampleFormat::Float,
            };
            let wav_spec = WavSpec {
                channels: 2,
                sample_rate: self.spec.sample_rate,
                bits_per_sample: bit_per_sample,
                sample_format,
            };
            let writer = hound::WavWriter::create(file_name, wav_spec).unwrap();

            self.writer = Some(writer);
            return Ok(Pmt::Bool(true));
        };
    }

    #[message_handler]
    fn spec_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let spec: BaseBandSpec = match p {
            Pmt::Any(b) => b.downcast_ref::<BaseBandSpec>().unwrap().clone(),
            _ => Default::default(),
        };
        self.spec = spec;
        return Ok(Pmt::Ok);
    }

    #[message_handler]
    fn duration_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        _p: Pmt,
    ) -> Result<Pmt> {
        if let Some(writer) = self.writer.as_ref() {
            let duration_secs = writer.duration() / self.spec.sample_rate;
            return Ok(Pmt::F32(duration_secs as f32));
        } else {
            return Ok(Pmt::F32(0.0));
        };
    }
}

#[async_trait]
impl Kernel for BaseBandSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let items = i.len();
        if items > 0 {
            if let Some(writer) = self.writer.as_mut() {
                for t in i {
                    match self.spec.format {
                        BaseBandFormat::f32 => {
                            writer.write_sample(t.re).unwrap();
                            writer.write_sample(t.im).unwrap();
                        }
                        BaseBandFormat::i16 => {
                            writer
                                .write_sample((t.re * i16::MAX as f32) as i16)
                                .unwrap();
                            writer
                                .write_sample((t.im * i16::MAX as f32) as i16)
                                .unwrap();
                        }
                        BaseBandFormat::i8 => {
                            writer.write_sample((t.re * i8::MAX as f32) as i8).unwrap();
                            writer.write_sample((t.im * i8::MAX as f32) as i8).unwrap();
                        }
                    }
                }
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(items);
        Ok(())
    }
}
