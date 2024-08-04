use baseband_sink::BaseBandSpec;
use iced::theme::Palette;
use iced::widget::{button, column, container, pick_list, row, slider, text, text_input, toggler};
use iced::{executor, Background, Color, Padding};
use iced::{Application, Command, Element, Length, Settings, Subscription, Theme};

mod baseband_sink;
mod sdr_device;
mod tail_sink;

mod sdr;
use iced_aw::menu::{self, Item, Menu, StyleSheet};
use iced_aw::{menu_bar, menu_items};
use sdr::*;

mod freq_chart;
use freq_chart::*;

mod waterfall;

mod utills;
use utills::*;
use waterfall::WaterFall;

const FFT_AMMOUNT: usize = 4096;
const STARTING_FREQ_IN_HZ: f64 = 100_000_000.0;
const UPS: u64 = 60;

struct RustcSdrSate {
    sdr_running: ToggleOption,
    selected_sdr: String,
    avalibale_sdrs: Vec<String>,
    recording: ToggleOption,
    sdr: Option<Sdr>,

    fft_update_rate: u64,
    fft_avg_num: usize,
    center_freq_val: Freq,
    center_freq: String,
    freq_unit: FreqUnits,
    gain: f64,
    sammple_rate_val: Freq,
    sammple_rate: SampleRates,

    chart: FreqChart,

    waterfall: WaterFall,
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
    WindowResize((u32, u32)),
    FftAvgChanged(usize),
    FftRateChanged(usize),
}

fn get_sdr_names() -> Vec<String> {
    let mut avalibale_sdrs: Vec<String> = Vec::new();
    for (idx, dev) in sdr_device::get_devices().unwrap().iter().enumerate() {
        avalibale_sdrs.push(idx.to_string() + " | " + &sdr_device::get_name(dev));
    }

    avalibale_sdrs
}

impl Application for RustcSdrSate {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_flags: ()) -> (RustcSdrSate, Command<Self::Message>) {
        let avalibale_sdrs = get_sdr_names();

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

                fft_update_rate: UPS,
                fft_avg_num: 10,
                center_freq_val: Freq::new(STARTING_FREQ_IN_HZ),
                center_freq: STARTING_FREQ_IN_HZ.to_string(),
                freq_unit: FreqUnits::Hz,
                gain: 0.0,
                sammple_rate_val: Freq::new(250_000f64),
                sammple_rate: SampleRates::S250k,

                chart: FreqChart::new(),

                waterfall: WaterFall::new(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Rustic SDR")
    }

    fn view(&self) -> Element<Message> {
        let menu_tpl_1 = |items| Menu::new(items).max_width(180.0).offset(10.0).spacing(5.0);
        let mb = menu_bar!((
            text("FFT Settings"),
            menu_tpl_1(menu_items!((row!(
                text("Average Num "),
                button(text("<")).on_press(Message::FftAvgChanged(0)),
                container(text(self.fft_avg_num.to_string())).padding(2),
                button(text(">")).on_press(Message::FftAvgChanged(1))
            )
            .align_items(iced::Alignment::Center))(
                row!(
                    text("Update Rate "),
                    button(text("<")).on_press(Message::FftRateChanged(0)),
                    container(text(self.fft_update_rate.to_string())).padding(2),
                    button(text(">")).on_press(Message::FftRateChanged(1))
                )
                .align_items(iced::Alignment::Center)
            )(row!(
                text("FFT Max "),
                slider(
                    std::ops::RangeInclusive::new(-125.0, 150.0),
                    self.chart.fft_max,
                    Message::FftMaxChanged
                )
            ))(row!(
                text("FFT Min "),
                slider(
                    std::ops::RangeInclusive::new(-125.0, 150.0),
                    self.chart.fft_min,
                    Message::FftMinChanged
                )
            ))))
        ))
        .draw_path(menu::DrawPath::Backdrop)
        .style(|theme: &iced::Theme| {
            let mut menu_app = theme.appearance(&Default::default());
            let mut menu_color = theme.palette().background;
            menu_color.r += 0.05;
            menu_color.g += 0.05;
            menu_color.b += 0.05;
            menu_app.menu_background = Background::Color(menu_color);
            menu_app.bar_background = Background::Color(menu_color);

            menu_app
        });
        let menus = row!(mb.width(Length::Fill)).align_items(iced::Alignment::Center);

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
                        std::ops::RangeInclusive::new(0.0, 1000.0),
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

        let chart_elements = container(column![self.chart.view(),]);

        column![
            menus,
            freq_elements.padding(Padding {
                top: 10.0,
                bottom: 0.0,
                left: 0.0,
                right: 0.0
            }),
            chart_elements,
            self.waterfall.view()
        ]
        .into()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Tick => {
                if let Some(dev) = self.sdr.as_mut() {
                    if let Ok(sample) = dev.get_preview_smaple() {
                        for (idx, val) in sample.iter().enumerate() {
                            self.chart.vals[idx] = *val;
                        }

                        self.waterfall
                            .add_line(&sample, self.chart.fft_max, self.chart.fft_min);
                    }

                    if self.recording.toggled {
                        let secs = dev.get_record_duration().unwrap() as u32;
                        let hours = secs / (60 * 60);
                        let sec_left = secs - (hours * 60 * 60);
                        let time = format!(
                            "Recording: {:02}:{:02}:{:02}",
                            hours,
                            sec_left / 60,
                            sec_left % 60
                        );
                        self.recording.label = Some(time);
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
                        dev.toggle_recording(
                            BaseBandSpec {
                                format: baseband_sink::BaseBandFormat::i16,
                                sample_rate: self.sammple_rate_val.get_hz() as u32,
                            },
                            &self.center_freq_val,
                        );
                        self.recording.toggled = true;
                    } else {
                        dev.toggle_recording(
                            BaseBandSpec {
                                format: baseband_sink::BaseBandFormat::i16,
                                sample_rate: self.sammple_rate_val.get_hz() as u32,
                            },
                            &self.center_freq_val,
                        );
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
                    if let Some(dev) = self.sdr.as_mut() {
                        let _ = dev.set_freq(self.center_freq_val.clone());
                    }
                }
                self.center_freq = new_freq_str;
            }
            Message::ToggleSdr(toggle) => {
                if let Some(dev) = self.sdr.as_mut() {
                    if self.recording.toggled {
                        dev.toggle_recording(
                            BaseBandSpec {
                                format: baseband_sink::BaseBandFormat::i16,
                                sample_rate: self.sammple_rate_val.get_hz() as u32,
                            },
                            &self.center_freq_val,
                        );
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
                            &sdr_device::get_devices().unwrap()[dev_num],
                            self.center_freq_val.clone(),
                            self.sammple_rate_val.clone(),
                            self.gain,
                            self.fft_avg_num,
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
                self.avalibale_sdrs = get_sdr_names();
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
            Message::WindowResize((_width, height)) => {
                self.waterfall.height = height as usize;
            }
            Message::FftAvgChanged(ammount) => {
                match ammount {
                    //sub
                    0 => {
                        if self.fft_avg_num > 1 {
                            self.fft_avg_num -= 1;
                        }
                    }
                    //add
                    1 => {
                        self.fft_avg_num += 1;
                    }
                    _ => {
                        panic!("Invalid FftAvgChanged message {:?}", ammount);
                    }
                }

                if let Some(dev) = self.sdr.as_ref() {
                    dev.set_fft_avg(self.fft_avg_num);
                }
            }
            Message::FftRateChanged(ammount) => {
                match ammount {
                    //sub
                    0 => {
                        if self.fft_update_rate > 1 {
                            self.fft_update_rate -= 1;
                        }
                    }
                    //add
                    1 => {
                        self.fft_update_rate += 1;
                    }
                    _ => {
                        panic!("Invalid FftRateChanged message {:?}", ammount);
                    }
                }
            }
        }

        Command::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let tick = iced::time::every(iced::time::Duration::from_millis(
            1000 / self.fft_update_rate,
        ))
        .map(|_| Message::Tick);
        let event = iced::event::listen_with(|event, _| match event {
            iced::Event::Window(_, iced::window::Event::Resized { width, height }) => {
                Some(Message::WindowResize((width, height)))
            }
            _ => None,
        });

        Subscription::batch(vec![tick, event])
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
