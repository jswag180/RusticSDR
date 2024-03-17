use iced::theme::Palette;
use iced::widget::{button, column, container, pick_list, row, slider, text, text_input, toggler};
use iced::{executor, Color};
use iced::{Application, Command, Element, Length, Settings, Subscription, Theme};
use rustfft::num_complex::ComplexFloat;
use rustfft::num_traits::Pow;
use std::sync::Arc;

mod sdr;
use sdr::*;

mod freq_chart;
use freq_chart::*;

mod utills;
use utills::*;

const FFT_AMMOUNT: usize = 1024;
const STARTING_FREQ_IN_HZ: f64 = 100_000_000.0;
const FPS: u64 = 60;

struct FftDsp {
    planner: rustfft::FftPlanner<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl FftDsp {
    pub fn new(fft_size: usize) -> Self {
        let mut planner = rustfft::FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);

        FftDsp { planner, fft }
    }
}

struct RustcSdrSate {
    sdr_running: ToggleOption,
    selected_sdr: String,
    avalibale_sdrs: Vec<String>,
    recording: ToggleOption,
    sdr: Option<Sdr>,

    center_freq_val: Freq,
    center_freq: String,
    freq_unit: FreqUnits,
    gain: f64,
    sammple_rate_val: Freq,
    sammple_rate: SampleRates,

    chart: FreqChart,
    fft: FftDsp,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    Unit(FreqUnits),
    ToggleRecord(bool),
    FreqChanged(String),
    ToggleSdr(bool),
    SelectSdr(String),
    RefreshSdrs,
    ChangeGain(f64),
    SammpleRate(SampleRates),
    FftMaxChanged(f32),
    FftMinChanged(f32),
}

impl Application for RustcSdrSate {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_flags: ()) -> (RustcSdrSate, Command<Self::Message>) {
        let avalibale_sdrs: Vec<String> = Sdr::get_sdrs();

        (
            RustcSdrSate {
                sdr_running: ToggleOption {
                    label: Some("SDR Running".into()),
                    toggled: false,
                },
                selected_sdr: avalibale_sdrs
                    .first()
                    .unwrap_or(&"".to_string())
                    .to_string(),
                avalibale_sdrs,
                recording: ToggleOption {
                    label: Some("Recording".into()),
                    toggled: false,
                },
                sdr: None,

                center_freq_val: Freq::new(STARTING_FREQ_IN_HZ),
                center_freq: "100000000.0".into(),
                freq_unit: FreqUnits::Hz,
                gain: 0.0,
                sammple_rate_val: Freq::new(250_000f64),
                sammple_rate: SampleRates::S250k,

                chart: FreqChart::new(),
                fft: FftDsp::new(FFT_AMMOUNT),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Rustic SDR")
    }

    fn view(&self) -> Element<Message> {
        let freq_elements = container(row!(column![
            column![row!(
                column![toggler(
                    self.sdr_running.label.clone(),
                    self.sdr_running.toggled,
                    Message::ToggleSdr
                )
                .width(Length::Shrink)]
                .padding(5),
                column![toggler(
                    self.recording.label.clone(),
                    self.recording.toggled,
                    Message::ToggleRecord
                )
                .width(Length::Shrink)]
                .padding(5),
                column![iced::widget::Rule::vertical(5)]
                    .height(30)
                    .padding(10),
                pick_list(
                    self.avalibale_sdrs.clone(),
                    Some(&self.selected_sdr),
                    Message::SelectSdr
                ),
                button("Refresh").on_press(Message::RefreshSdrs),
                row!(
                    text("Gain: "),
                    slider(
                        std::ops::RangeInclusive::new(0.0, 49.0),
                        self.gain,
                        Message::ChangeGain
                    )
                )
                .padding(5),
                pick_list(
                    &SampleRates::ALL[..],
                    Some(self.sammple_rate),
                    Message::SammpleRate
                ),
            )],
            row!(
                column![
                    text_input("Enter a freq", &self.center_freq).on_input(Message::FreqChanged),
                ]
                .align_items(iced::Alignment::Center)
                .width(Length::Fill),
                column![pick_list(
                    &FreqUnits::ALL[..],
                    Some(self.freq_unit),
                    Message::Unit
                )]
            )
        ],));

        let chart_elements = container(column![
            row!(
                row!(
                    text("FFT Max: "),
                    slider(
                        std::ops::RangeInclusive::new(-125.0, 150.0),
                        self.chart.fft_max,
                        Message::FftMaxChanged
                    )
                )
                .padding(5),
                row!(
                    text("FFT Min: "),
                    slider(
                        std::ops::RangeInclusive::new(-125.0, 150.0),
                        self.chart.fft_min,
                        Message::FftMinChanged
                    )
                )
                .padding(5),
            ),
            self.chart.view(),
        ]);

        column![freq_elements, chart_elements,].into()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick => {
                if let Some(dev) = self.sdr.as_mut() {
                    if let Ok(mut sample) = dev.get_preview_smaple() {
                        self.fft.fft.process(&mut sample);
                        for i in 0..self.chart.vals.len() {
                            let mut index = i;
                            if (i / (FFT_AMMOUNT / 2)) == 0 {
                                index += FFT_AMMOUNT / 2;
                            } else {
                                index -= FFT_AMMOUNT / 2;
                            }
                            let psd_log = 10.0
                                * f32::log10(
                                    sample[index].pow(2.0).abs()
                                        / (FFT_AMMOUNT as f32
                                            / self.sammple_rate_val.get_hz() as f32)
                                        + 1.0, //This +1.0 is need to stop (over/under)flows on debug
                                );
                            self.chart.vals[i] = psd_log;
                        }
                    }
                }
            }
            Message::Unit(new_unit) => {
                let new = match new_unit {
                    FreqUnits::Hz => self.center_freq_val.get_hz(),
                    FreqUnits::KHz => self.center_freq_val.get_khz(),
                    FreqUnits::MHz => self.center_freq_val.get_mhz(),
                    FreqUnits::GHz => self.center_freq_val.get_ghz(),
                };
                self.center_freq = new.to_string();
                self.freq_unit = new_unit;
            }
            Message::ToggleRecord(toggle) => {
                if let Some(dev) = self.sdr.as_mut() {
                    if toggle {
                        dev.toggle_recording();
                        self.recording.toggled = true;
                    } else {
                        dev.toggle_recording();
                        self.recording.toggled = false;
                    }
                }
            }
            Message::FreqChanged(new_freq_str) => {
                if let Ok(new_freq) = new_freq_str.parse::<f64>() {
                    match self.freq_unit {
                        FreqUnits::Hz => self.center_freq_val.set_hz(new_freq),
                        FreqUnits::KHz => self.center_freq_val.set_khz(new_freq),
                        FreqUnits::MHz => self.center_freq_val.set_mhz(new_freq),
                        FreqUnits::GHz => self.center_freq_val.set_ghz(new_freq),
                    }
                    if let Some(dev) = self.sdr.as_ref() {
                        dev.set_freq(self.center_freq_val.clone());
                    }
                }
                self.center_freq = new_freq_str;
            }
            Message::ToggleSdr(toggle) => {
                if let Some(dev) = self.sdr.as_ref() {
                    if self.recording.toggled {
                        dev.toggle_recording();
                        self.recording.toggled = false;
                    }

                    self.sdr = None;
                    self.sdr_running.toggled = toggle;
                } else {
                    if !self.selected_sdr.is_empty() {
                        let dev_num: usize = self
                            .selected_sdr
                            .split(" | ")
                            .next()
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();

                        self.sdr = Some(Sdr::new(
                            dev_num,
                            self.center_freq_val.clone(),
                            self.sammple_rate_val.clone(),
                            self.gain,
                        ));
                    } else {
                        return Command::none();
                    }

                    self.sdr_running.toggled = toggle;
                }
            }
            Message::SelectSdr(sdr_name) => {
                if !self.sdr_running.toggled {
                    self.selected_sdr = sdr_name;
                }
            }
            Message::RefreshSdrs => {
                self.avalibale_sdrs = Sdr::get_sdrs();
                self.selected_sdr = self
                    .avalibale_sdrs
                    .first()
                    .unwrap_or(&"".to_string())
                    .to_string();
            }
            Message::ChangeGain(new_gain) => {
                self.gain = new_gain;

                if let Some(dev) = self.sdr.as_mut() {
                    dev.set_gain(self.gain);
                }
            }
            Message::SammpleRate(new_rate) => {
                if !self.sdr_running.toggled {
                    let mut new_freq = Freq::new(0f64);
                    match new_rate {
                        SampleRates::S250k => new_freq.set_khz(250f64),
                        SampleRates::S1024m => new_freq.set_mhz(1.024),
                        SampleRates::S1536m => new_freq.set_mhz(1.536),
                        SampleRates::S1792m => new_freq.set_mhz(1.792),
                        SampleRates::S192m => new_freq.set_mhz(1.192),
                        SampleRates::S2048m => new_freq.set_mhz(2.048),
                        SampleRates::S216m => new_freq.set_mhz(2.16),
                        SampleRates::S24m => new_freq.set_mhz(2.4),
                        SampleRates::S256m => new_freq.set_mhz(2.56),
                        SampleRates::S288m => new_freq.set_mhz(2.88),
                        SampleRates::S32m => new_freq.set_mhz(3.2),
                    }
                    self.sammple_rate_val = new_freq;
                    self.sammple_rate = new_rate;
                }
            }
            Message::FftMaxChanged(new_max) => {
                self.chart.fft_max = new_max;
            }
            Message::FftMinChanged(new_min) => {
                self.chart.fft_min = new_min;
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        iced::time::every(iced::time::Duration::from_millis(1000 / FPS)).map(|_| Message::Tick)
    }

    fn theme(&self) -> Self::Theme {
        let mut pal = Palette::DRACULA;
        pal.background = Color::from_rgb8(10, 25, 10);
        pal.primary = Color::from_rgb8(214, 81, 8);
        Theme::custom("defualt".into(), pal)
    }
}

fn main() {
    let _ = RustcSdrSate::run(Settings::default());
}
