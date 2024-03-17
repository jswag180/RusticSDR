use chrono::prelude::*;
use ringbuf::ring_buffer;
use rustfft::num_complex::{Complex32, ComplexFloat};
use soapysdr::Direction::Rx;
use std::fs::File;
use std::io::BufWriter;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

struct WavFile {
    wav: hound::WavSpec,
    writer: hound::WavWriter<BufWriter<File>>,
}

#[derive(Debug, Default, Clone)]
pub struct Freq(f64);

impl Freq {
    pub fn set_hz(&mut self, f: f64) {
        self.0 = f;
    }

    pub fn get_hz(&self) -> f64 {
        self.0
    }

    pub fn set_khz(&mut self, f: f64) {
        self.0 = f * 1_000.0;
    }

    pub fn get_khz(&self) -> f64 {
        self.0 / 1_000.0
    }

    pub fn set_mhz(&mut self, f: f64) {
        self.0 = f * 1_000_000.0;
    }

    pub fn get_mhz(&self) -> f64 {
        self.0 / 1_000_000.0
    }

    pub fn set_ghz(&mut self, f: f64) {
        self.0 = f * 1_000_000_000.0;
    }

    pub fn get_ghz(&self) -> f64 {
        self.0 / 1_000_000_000.0
    }
}

impl ToString for Freq {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Freq {
    pub fn new(val: f64) -> Self {
        Freq(val)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreqUnits {
    Hz,
    KHz,
    MHz,
    GHz,
}

impl FreqUnits {
    pub const ALL: [FreqUnits; 4] = [
        FreqUnits::Hz,
        FreqUnits::KHz,
        FreqUnits::MHz,
        FreqUnits::GHz,
    ];
}

impl std::fmt::Display for FreqUnits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FreqUnits::Hz => "Hz",
                FreqUnits::KHz => "KHz",
                FreqUnits::MHz => "MHz",
                FreqUnits::GHz => "GHz",
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRates {
    S250k,
    S1024m,
    S1536m,
    S1792m,
    S192m,
    S2048m,
    S216m,
    S24m,
    S256m,
    S288m,
    S32m,
}

impl SampleRates {
    pub const ALL: [SampleRates; 11] = [
        SampleRates::S250k,
        SampleRates::S1024m,
        SampleRates::S1536m,
        SampleRates::S1792m,
        SampleRates::S192m,
        SampleRates::S2048m,
        SampleRates::S216m,
        SampleRates::S24m,
        SampleRates::S256m,
        SampleRates::S288m,
        SampleRates::S32m,
    ];
}

impl std::fmt::Display for SampleRates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                SampleRates::S250k => "250 ksps",
                SampleRates::S1024m => "1.024 msps",
                SampleRates::S1536m => "1.536 msps",
                SampleRates::S1792m => "1.792 msps",
                SampleRates::S192m => "1.92 msps",
                SampleRates::S2048m => "2.048 msps",
                SampleRates::S216m => "2.16 mmsps",
                SampleRates::S24m => "2.4 msps",
                SampleRates::S256m => "2.56 msps",
                SampleRates::S288m => "2.88 msps",
                SampleRates::S32m => "3.2 msps",
            }
        )
    }
}

pub struct Sdr {
    pub ring_buf: ringbuf::Consumer<
        [Complex32; super::FFT_AMMOUNT],
        Arc<
            ringbuf::SharedRb<
                [Complex32; super::FFT_AMMOUNT],
                Vec<std::mem::MaybeUninit<[Complex32; super::FFT_AMMOUNT]>>,
            >,
        >,
    >,
    pub send: Sender<SdrMessage>,
    pub recv: Receiver<SdrReply>,
    sdr_thread: JoinHandle<()>,
}

pub enum SdrMessage {
    SetFreq(Freq),
    SetGain(f64),
    ToggleRecord,
    Stop,
}

//TODO use this to send errors like timeouts, overflows, and failure to change sdr params to the ui
//Also it can send back recording status
pub enum SdrReply {
    Unk,
}

impl Sdr {
    pub fn new(sdr_num: usize, center_freq: Freq, sample_rate: Freq, gain: f64) -> Self {
        let (thread_tx, thread_rx) = mpsc::channel::<SdrMessage>();
        let (tx, rx) = mpsc::channel::<SdrReply>();

        let send = thread_tx.clone();

        let (mut ring_p, ring_c) =
            ring_buffer::SharedRb::<[Complex32; super::FFT_AMMOUNT], Vec<_>>::new(2).split();

        let th = thread::spawn(move || {
            let mut recording_wav: Option<WavFile> = None;

            let dev = Sdr::get_sdr(sdr_num).expect("Could not get requested sdr!");

            dev.set_frequency(Rx, 0, center_freq.get_hz(), ()).unwrap();
            dev.set_sample_rate(Rx, 0, sample_rate.get_hz()).unwrap();
            dev.set_gain(Rx, 0, gain).unwrap();

            let mut stream = dev.rx_stream::<Complex32>(&[0]).unwrap();
            stream.activate(None).unwrap();

            let mut stream_buf = vec![Complex32::default(); stream.mtu().unwrap()];

            loop {
                //TODO gracefully handle timeouts/overflows
                let stream_read_len = stream
                    .read(&mut [&mut stream_buf[..]], 1_000_000)
                    .expect("read failed");

                // This should probably use stream_read_len / FFT_AMMOUNT
                for i in 0..(stream_buf.len() / super::FFT_AMMOUNT) {
                    let streamm_slice: [Complex32; super::FFT_AMMOUNT] = stream_buf
                        [i * super::FFT_AMMOUNT..(i * super::FFT_AMMOUNT) + super::FFT_AMMOUNT]
                        .try_into()
                        .expect("slice with incorrect length");
                    let _ = ring_p.push(streamm_slice);
                }

                if let Ok(msg) = thread_rx.try_recv() {
                    match msg {
                        SdrMessage::SetGain(gain) => dev.set_gain(Rx, 0, gain).unwrap(),
                        SdrMessage::ToggleRecord => match recording_wav.as_mut() {
                            Some(_) => {
                                recording_wav = None;
                            }
                            None => {
                                recording_wav = Some(Sdr::create_wav_file(
                                    sample_rate.clone(),
                                    dev.frequency(Rx, 0).unwrap_or_default(),
                                ));
                            }
                        },
                        SdrMessage::Stop => {
                            stream.deactivate(None).unwrap();
                            break;
                        }
                        SdrMessage::SetFreq(new_freq) => {
                            let _ = dev.set_frequency(Rx, 0, new_freq.get_hz(), ());
                        }
                    }
                }

                if let Some(wav_file) = recording_wav.as_mut() {
                    for sample in stream_buf.iter().take(stream_read_len) {
                        let _ = wav_file
                            .writer
                            .write_sample((sample.re() * i16::MAX as f32) as i16);
                        let _ = wav_file
                            .writer
                            .write_sample((sample.im() * i16::MAX as f32) as i16);
                    }
                    let _ = wav_file.writer.flush();
                }
            }
        });

        Sdr {
            ring_buf: ring_c,
            send,
            recv: rx,
            sdr_thread: th,
        }
    }

    #[inline]
    pub fn get_preview_smaple(&mut self) -> Result<[Complex32; super::FFT_AMMOUNT], ()> {
        match self.ring_buf.pop() {
            Some(vec) => Ok(vec),
            None => Err(()),
        }
    }

    pub fn set_freq(&self, freq: Freq) {
        let _ = self.send.send(SdrMessage::SetFreq(freq));
    }

    pub fn set_gain(&self, gain: f64) {
        let _ = self.send.send(SdrMessage::SetGain(gain));
    }

    pub fn get_sdrs() -> Vec<String> {
        let mut sdrs: Vec<String> = Vec::new();
        let sdr_args = soapysdr::enumerate("").expect("Error listing devices");

        for (dev_num, sdr) in sdr_args.into_iter().enumerate() {
            let dev_string = dev_num.to_string() + " | " + sdr.get("label").unwrap();
            sdrs.push(dev_string);
        }

        sdrs
    }

    pub fn get_sdr(num: usize) -> Result<soapysdr::Device, ()> {
        for (device_num, device) in soapysdr::enumerate("")
            .expect("Error listing devices")
            .into_iter()
            .enumerate()
        {
            if num == device_num {
                return Ok(soapysdr::Device::new(device).expect("Error opening device!"));
            }
        }
        Err(())
    }

    fn create_wav_file(sample_rate: Freq, starting_freq: f64) -> WavFile {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sample_rate.get_hz() as u32,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let time_stamp = Utc::now();

        let writer = hound::WavWriter::create(
            format!(
                "baseband_{}Hz_{}-{}-{}_{}-{}-{}.wav",
                starting_freq,
                time_stamp.hour(),
                time_stamp.minute(),
                time_stamp.second(),
                time_stamp.month(),
                time_stamp.day(),
                time_stamp.year()
            ),
            spec,
        )
        .unwrap();

        WavFile { wav: spec, writer }
    }

    pub fn toggle_recording(&self) {
        let _ = self.send.send(SdrMessage::ToggleRecord);
    }
}

impl Drop for Sdr {
    fn drop(&mut self) {
        if !self.sdr_thread.is_finished() {
            self.send.send(SdrMessage::Stop).unwrap();
        }
    }
}
