use iced::{Element, Length};
use plotters::{coord::Shift, prelude::*};
use plotters_backend::DrawingBackend;
use plotters_iced::{plotters_backend, Chart, ChartBuilder, ChartWidget, DrawingArea};

pub struct FreqChart {
    pub vals: Vec<f32>,
    pub fft_max: f32,
    pub fft_min: f32,
}

impl FreqChart {
    pub fn new() -> Self {
        Self {
            vals: vec![0.0; super::FFT_AMMOUNT],
            fft_max: 90f32,
            fft_min: 0f32,
        }
    }

    pub fn view(&self) -> Element<super::Message> {
        let chart = ChartWidget::new(self)
            .width(Length::Fill)
            .height(Length::Fill);

        chart.into()
    }
}

impl Chart<super::Message> for FreqChart {
    type State = ();
    // leave it empty
    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, _builder: ChartBuilder<DB>) {}

    fn draw_chart<DB: DrawingBackend>(&self, _state: &Self::State, root: DrawingArea<DB, Shift>) {
        let ch = root.split_evenly((1, 1));
        draw_chart(
            ChartBuilder::on(&ch[0]),
            &self.vals,
            self.fft_max,
            self.fft_min,
        );
    }
}

fn draw_chart<DB: DrawingBackend>(
    mut chart: ChartBuilder<DB>,
    sammples: &[f32],
    max: f32,
    min: f32,
) {
    let mut chart = chart
        .build_cartesian_2d(0f32..super::FFT_AMMOUNT as f32, min..max)
        .unwrap();

    chart
        .draw_series(LineSeries::new(
            (0..super::FFT_AMMOUNT)
                .map(|x| x as f32)
                .map(|x| (x, sammples[x as usize])),
            &full_palette::ORANGE,
        ))
        .unwrap();
}
