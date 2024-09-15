use std::collections::VecDeque;

use iced::{
    widget::{column, container, image, image::Handle, Column},
    Element, Length,
};

use crate::FFT_AMMOUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pallet {
    Turbo,
    Magma,
    Plasma,
    Spectral,
    Rainbow,
}

impl std::fmt::Display for Pallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Pallet::Turbo => "Turbo",
                Pallet::Magma => "Magma",
                Pallet::Plasma => "Plasma",
                Pallet::Spectral => "Spectral",
                Pallet::Rainbow => "Rainbow",
            }
        )
    }
}

pub struct WaterFall {
    pub height: usize,
    pub handels: VecDeque<Handle>,
    pub pallet: Pallet,
}

impl WaterFall {
    pub fn new() -> Self {
        Self {
            height: 0,
            pallet: Pallet::Turbo,
            handels: VecDeque::new(),
        }
    }

    pub fn add_line(&mut self, sample: &[f32], max: f32, min: f32) {
        let mut new_data: Vec<u8> = Vec::with_capacity(FFT_AMMOUNT * 4);
        for val in sample {
            let adj_val = val.clamp(min, max) / max;
            let pallet = match self.pallet {
                Pallet::Turbo => colorgrad::turbo(),
                Pallet::Magma => colorgrad::magma(),
                Pallet::Plasma => colorgrad::plasma(),
                Pallet::Spectral => colorgrad::spectral(),
                Pallet::Rainbow => colorgrad::rainbow(),
            };
            let color = pallet.at(adj_val.into()).to_rgba8();
            let pix: Vec<u8> = vec![color[0], color[1], color[2], color[3]];

            new_data.extend(pix);
        }

        while self.handels.len() >= self.height {
            let old_handle = self.handels.pop_back();
            drop(old_handle);
        }

        self.handels
            .push_front(Handle::from_pixels(FFT_AMMOUNT as u32, 1, new_data));
    }

    pub fn view(&self) -> Element<super::Message> {
        let mut waterfall_display = Column::new();
        for i in self.handels.iter() {
            waterfall_display = waterfall_display.push(
                image(i.clone())
                    .content_fit(iced::ContentFit::Fill)
                    .width(Length::Fill),
            );
        }
        let water_fall_elements = container(column![waterfall_display
            .height(Length::Fill)
            .width(Length::Fill)]);

        water_fall_elements.into()
    }
}
