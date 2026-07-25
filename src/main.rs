//! A shelf of ZX Spectrum ports, drawn with macroquad.
//!
//! The games live in their own crates and know nothing about windows. This
//! binary owns the machine: it reads the keyboard, ticks whatever is playing at
//! the Spectrum's 17 frames per second, blits the resulting 256x192 screen
//! scaled up to the window, and shows the picker in between.

mod debug;
mod handover;
mod menu;

use handover::Handover;
use macroquad::prelude::*;
use menu::{CATALOGUE, Choice, Menu};
use speccy::{Cartridge, Frame, HEIGHT, Input, PALETTE, WIDTH};
use speccy_audio::Beeper;

/// Everything here runs at the Spectrum's own pace: 17 frames per second.
const FRAMES_PER_SECOND: f32 = mm_core::FRAMES_PER_SECOND;

/// The flash attribute swaps ink and paper roughly three times a second.
const FLASH_PERIOD: f32 = 16.0 / 50.0;

/// What the shell is showing.
enum Screen {
    Picker(Menu),
    Playing(Box<dyn Cartridge>),
}

fn window() -> Conf {
    Conf {
        window_title: "Speccy Arcade".to_owned(),
        window_width: WIDTH as i32 * 3,
        window_height: HEIGHT as i32 * 3,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window)]
async fn main() {
    let debug_keys = debug::enabled();
    let mut screen = Screen::Picker(Menu::new(0));
    // The catalogue row to come back to when a game is put away.
    let mut playing = 0usize;
    let mut running = true;
    // Enter and Escape both change the screen, so neither may act on the one
    // it arrives at until it has been let go.
    let mut handover = Handover::default();

    let beeper = Beeper::new();
    let mut frame = Frame::new();

    let image = Image::gen_image_color(WIDTH as u16, HEIGHT as u16, BLACK);
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);

    let mut tick_accumulator = 0.0f32;
    let mut flash_timer = 0.0f32;
    let mut flash_on = false;

    while running {
        let delta = get_frame_time().min(0.25);

        flash_timer += delta;
        if flash_timer >= FLASH_PERIOD {
            flash_timer -= FLASH_PERIOD;
            flash_on = !flash_on;
        }

        if let Screen::Playing(game) = &mut screen
            && debug_keys
        {
            debug::read_keys(game.as_mut());
        }

        // Run on a fixed clock, however fast the window redraws.
        tick_accumulator += delta;
        let step = 1.0 / FRAMES_PER_SECOND;
        while tick_accumulator >= step {
            tick_accumulator -= step;

            let input = handover.filter(read_input());
            match &mut screen {
                Screen::Picker(picker) => match picker.update(input) {
                    Choice::Stay => {}
                    Choice::Play(index) => {
                        playing = index;
                        let launch = CATALOGUE[index]
                            .launch
                            .expect("the picker only starts games it can start");
                        screen = Screen::Playing(launch());
                        handover.latch();
                    }
                    Choice::Quit => running = false,
                },
                Screen::Playing(game) => {
                    game.update(input);
                    for sound in game.sounds().drain() {
                        beeper.play(sound);
                    }
                    if game.finished() {
                        beeper.play(speccy::Sound::Silence);
                        screen = Screen::Picker(Menu::new(playing));
                        handover.latch();
                    }
                }
            }
        }

        let (memory, border) = match &screen {
            Screen::Picker(picker) => (picker.memory(), 0),
            Screen::Playing(game) => (game.memory(), game.border()),
        };
        frame.render(memory, flash_on);
        texture.update_from_bytes(WIDTH as u32, HEIGHT as u32, &frame.pixels);

        draw_scaled(&texture, border);
        next_frame().await;
    }
}

fn read_input() -> Input {
    Input {
        left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
        right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
        up: is_key_down(KeyCode::Up) || is_key_down(KeyCode::W),
        down: is_key_down(KeyCode::Down) || is_key_down(KeyCode::S),
        jump: is_key_down(KeyCode::Space) || is_key_down(KeyCode::Up),
        start: is_key_down(KeyCode::Enter),
        pause: is_key_down(KeyCode::P),
        mute: is_key_down(KeyCode::M),
        back: is_key_down(KeyCode::Q) || is_key_down(KeyCode::Escape),
    }
}

/// Draw the screen as large as fits, centred, with the border around it.
fn draw_scaled(texture: &Texture2D, border: u8) {
    let colour = PALETTE[(border & 7) as usize];
    clear_background(Color::from_rgba(colour[0], colour[1], colour[2], 255));

    let scale = (screen_width() / WIDTH as f32)
        .min(screen_height() / HEIGHT as f32)
        .floor()
        .max(1.0);
    let width = WIDTH as f32 * scale;
    let height = HEIGHT as f32 * scale;

    draw_texture_ex(
        texture,
        ((screen_width() - width) / 2.0).floor(),
        ((screen_height() - height) / 2.0).floor(),
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(width, height)),
            ..Default::default()
        },
    );
}
