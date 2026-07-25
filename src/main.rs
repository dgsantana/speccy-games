//! Manic Miner, drawn with macroquad.
//!
//! The engine lives in `mm-core` and knows nothing about windows. This binary
//! reads the keyboard, ticks the engine at the Spectrum's 17 frames per second,
//! and blits the resulting 256x192 screen scaled up to the window.

mod debug;

use macroquad::prelude::*;
use mm_audio::Beeper;
use mm_core::{FRAMES_PER_SECOND, Frame, Game, HEIGHT, Input, PALETTE, WIDTH};

/// The flash attribute swaps ink and paper roughly three times a second.
const FLASH_PERIOD: f32 = 16.0 / 50.0;

fn window() -> Conf {
    Conf {
        window_title: "Manic Miner".to_owned(),
        window_width: WIDTH as i32 * 3,
        window_height: HEIGHT as i32 * 3,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window)]
async fn main() {
    let debug_keys = debug::enabled();
    let mut game = Game::new();
    let mut frame = Frame::new();
    let beeper = Beeper::new();

    let image = Image::gen_image_color(WIDTH as u16, HEIGHT as u16, BLACK);
    let texture = Texture2D::from_image(&image);
    texture.set_filter(FilterMode::Nearest);

    let mut tick_accumulator = 0.0f32;
    let mut flash_timer = 0.0f32;
    let mut flash_on = false;

    while !game.quit {
        let delta = get_frame_time().min(0.25);

        if debug_keys {
            debug::read_keys(&mut game);
        }

        flash_timer += delta;
        if flash_timer >= FLASH_PERIOD {
            flash_timer -= FLASH_PERIOD;
            flash_on = !flash_on;
        }

        // Run the engine on its own fixed clock, however fast the window redraws.
        tick_accumulator += delta;
        let step = 1.0 / FRAMES_PER_SECOND;
        while tick_accumulator >= step {
            tick_accumulator -= step;
            game.update(read_input());
            for sound in game.sounds.drain() {
                beeper.play(sound);
            }
        }

        frame.render(&game.mem, flash_on);
        texture.update_from_bytes(WIDTH as u32, HEIGHT as u32, &frame.pixels);

        draw_scaled(&texture, game.border());
        next_frame().await;
    }
}

fn read_input() -> Input {
    Input {
        left: is_key_down(KeyCode::Left) || is_key_down(KeyCode::A),
        right: is_key_down(KeyCode::Right) || is_key_down(KeyCode::D),
        jump: is_key_down(KeyCode::Space) || is_key_down(KeyCode::Up),
        start: is_key_down(KeyCode::Enter),
        pause: is_key_down(KeyCode::P),
        mute: is_key_down(KeyCode::M),
        quit: is_key_down(KeyCode::Q) || is_key_down(KeyCode::Escape),
    }
}

/// Draw the screen as large as fits, centred, with the cavern's border around it.
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
