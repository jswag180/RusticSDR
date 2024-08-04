use futuresdr::anyhow::Ok;
use futuresdr::blocks::seify::SourceBuilder;
use futuresdr::blocks::{Apply, ApplyNM, Fft};
use futuresdr::macros::connect;
use futuresdr::num_complex::{Complex32, ComplexFloat};
use futuresdr::runtime::scheduler::SmolScheduler;
use futuresdr::runtime::{Flowgraph, FlowgraphHandle, Runtime};
use std::collections::VecDeque;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use crate::baseband_sink::{BaseBandSink, BaseBandSpec};
use crate::sdr_device::SdrLimits;
use crate::tail_sink::{TailRing, TailSink};
use crate::FFT_AMMOUNT;

static RT: LazyLock<Runtime<SmolScheduler>> = LazyLock::new(Runtime::new);

pub enum SdrError {
    FreqNotInRange,
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

impl std::fmt::Display for Freq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
    limits: SdrLimits,
    tail_ring: Arc<TailRing<f32>>,
    handle: FlowgraphHandle,
    fft_avg: Arc<AtomicUsize>,
    sdr_id: usize,
    freq_port_id: usize,
    gain_port_id: usize,

    bb_id: usize,
    toggle_port_id: usize,
    spec_port_id: usize,
    duration_port_id: usize,
}

impl Sdr {
    pub fn new(
        sdr_args: &futuresdr::seify::Args,
        center_freq: Freq,
        sample_rate: Freq,
        gain_percent: f64,
        fft_avg_num: usize,
    ) -> Self {
        let mut fg = Flowgraph::new();

        //SDR Soruce
        let (device, limits) = crate::sdr_device::new_sdr(sdr_args).unwrap();
        let gain = limits
            .gain_range
            .closest((gain_percent / 1000.0) * get_max_gain(&limits))
            .unwrap();
        let src = SourceBuilder::new()
            .device(device)
            .frequency(center_freq.get_hz())
            .sample_rate(sample_rate.get_hz())
            .gain(gain)
            .build()
            .unwrap();
        let freq_port_id = src
            .message_input_name_to_id("freq")
            .expect("No freq port found!");
        let gain_port_id = src
            .message_input_name_to_id("gain")
            .expect("No gain port found!");

        //Baseband
        let bb_sink = BaseBandSink::new();
        let toggle_port_id = bb_sink
            .message_input_name_to_id("toggle")
            .expect("No toggle port found!");
        let spec_port_id = bb_sink
            .message_input_name_to_id("spec")
            .expect("No spec port found!");
        let duration_port_id = bb_sink
            .message_input_name_to_id("duration")
            .expect("No duration port found!");

        //Preview window
        let mut window: [f32; FFT_AMMOUNT] = [0.0; FFT_AMMOUNT];
        for (idx, val) in window.iter_mut().enumerate() {
            *val = 0.5
                - (0.5
                    * f32::cos(
                        (2.0 * std::f32::consts::PI * idx as f32) / (FFT_AMMOUNT as f32 - 1.0),
                    ));
        }
        let hanning_window = ApplyNM::<_, _, _, FFT_AMMOUNT, FFT_AMMOUNT>::new(
            move |in_samples: &[Complex32], out_samples: &mut [Complex32]| {
                for (idx, val) in in_samples.iter().enumerate() {
                    out_samples[idx] = window[idx] * *val;
                }
            },
        );

        let fft = Fft::with_options(
            FFT_AMMOUNT,
            futuresdr::blocks::FftDirection::Forward,
            true,
            None,
        );

        let sample_rate_hz = sample_rate.get_hz() as f32;
        let psd = Apply::new(move |x: &Complex32| {
            10.0 * f32::log10(x.powi(2).abs() / (FFT_AMMOUNT as f32 / sample_rate_hz) + 1.0)
        });

        let fft_avg = Arc::new(AtomicUsize::new(fft_avg_num));
        let fft_avg_ref = fft_avg.clone();
        let window: Arc<Mutex<VecDeque<Vec<f32>>>> = Mutex::new(VecDeque::new()).into();
        let window_ref = window.clone();
        let avg_window = ApplyNM::<_, _, _, FFT_AMMOUNT, FFT_AMMOUNT>::new(
            move |in_samples: &[f32], out_samples: &mut [f32]| {
                let mut window = window_ref.lock().unwrap();
                let window_size = fft_avg_ref.load(std::sync::atomic::Ordering::Relaxed);
                while window.len() >= window_size {
                    let old_val: Option<Vec<f32>> = window.pop_back();
                    drop(old_val);
                }
                window.push_front(in_samples.to_vec());

                for (idx, sample) in out_samples.iter_mut().enumerate() {
                    *sample = 0.0;
                    for section in window.iter() {
                        *sample += section[idx] / window_size as f32;
                    }
                }
            },
        );

        let tail_ring = Arc::new(TailRing::<f32>::new(FFT_AMMOUNT));
        let tail_sink = TailSink::new(tail_ring.clone());

        let mut sdr_id = 0;
        let mut bb_id = 0;
        let con = || -> futuresdr::anyhow::Result<()> {
            connect!(fg, src > bb_sink);
            connect!(fg, src > hanning_window > fft > psd > avg_window > tail_sink);

            sdr_id = src;
            bb_id = bb_sink;

            futuresdr::anyhow::Result::Ok(())
        };
        con().unwrap();

        let (_res, handle) = RT.start_sync(fg);

        Sdr {
            limits,
            tail_ring,
            handle,
            fft_avg,
            sdr_id,
            freq_port_id,
            gain_port_id,

            bb_id,
            toggle_port_id,
            spec_port_id,
            duration_port_id,
        }
    }

    #[inline]
    pub fn get_preview_smaple(&mut self) -> Result<MutexGuard<Vec<f32>>, ()> {
        self.tail_ring.get()
    }

    pub fn get_record_duration(&mut self) -> Result<f32, futuresdr::anyhow::Error> {
        let res = futuresdr::async_io::block_on(self.handle.callback(
            self.bb_id,
            self.duration_port_id,
            futuresdr::runtime::Pmt::Ok,
        ))?;

        match res {
            futuresdr::runtime::Pmt::F32(val) => Ok(val),
            _ => Ok(0.0),
        }
    }

    pub fn set_freq(&mut self, freq: Freq) -> Result<(), SdrError> {
        if self.limits.freq_range.contains(freq.get_hz()) {
            return core::result::Result::Err(SdrError::FreqNotInRange);
        }

        let _ = futuresdr::async_io::block_on(self.handle.callback(
            self.sdr_id,
            self.freq_port_id,
            futuresdr::runtime::Pmt::F64(freq.get_hz()),
        ));

        core::result::Result::Ok(())
    }

    pub fn set_gain(&mut self, gain_percent: f64) {
        let gain = self
            .limits
            .gain_range
            .closest((gain_percent / 1000.0) * get_max_gain(&self.limits))
            .unwrap();

        let _ = futuresdr::async_io::block_on(self.handle.callback(
            self.sdr_id,
            self.gain_port_id,
            futuresdr::runtime::Pmt::F64(gain),
        ));
    }

    pub fn toggle_recording(&mut self, spec: BaseBandSpec, freq: &Freq) {
        let _ = futuresdr::async_io::block_on(self.handle.callback(
            self.bb_id,
            self.spec_port_id,
            futuresdr::runtime::Pmt::Any(Box::new(spec)),
        ));

        let _ = futuresdr::async_io::block_on(self.handle.callback(
            self.bb_id,
            self.toggle_port_id,
            futuresdr::runtime::Pmt::F64(freq.get_hz()),
        ));
    }

    pub fn set_fft_avg(&self, num: usize) {
        self.fft_avg
            .store(num, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Drop for Sdr {
    fn drop(&mut self) {
        let _ = futuresdr::async_io::block_on(self.handle.terminate_and_wait());
    }
}

fn get_max_gain(limits: &SdrLimits) -> f64 {
    let mut max_gain: f64 = 0.0;
    limits.gain_range.items.iter().for_each(|x| match x {
        futuresdr::seify::RangeItem::Interval(_start, stop) => {
            max_gain = max_gain.max(*stop);
        }
        futuresdr::seify::RangeItem::Value(val) => {
            max_gain = max_gain.max(*val);
        }
        futuresdr::seify::RangeItem::Step(_start, stop, _step) => {
            max_gain = max_gain.max(*stop);
        }
    });

    max_gain
}
