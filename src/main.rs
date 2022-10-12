pub mod bus;
pub mod cart;
pub mod cpu;
pub mod opcodes;
pub mod trace;
pub mod ppu;
pub mod graphics_data;
pub mod controller;

use bus::Bus;
use cart::Rom;
use cpu::Mem;
use cpu::CPU;
use trace::trace;
use graphics_data::frame::Frame;
use graphics_data::palette;
use ppu::MyPPU;
// use rand::Rng;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::EventPump;

use std::collections::HashMap;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate bitflags;

fn color(byte: u8) -> Color {
    match byte {
        0 => sdl2::pixels::Color::BLACK,
        1 => sdl2::pixels::Color::WHITE,
        2 | 9 => sdl2::pixels::Color::GREY,
        3 | 10 => sdl2::pixels::Color::RED,
        4 | 11 => sdl2::pixels::Color::GREEN,
        5 | 12 => sdl2::pixels::Color::BLUE,
        6 | 13 => sdl2::pixels::Color::MAGENTA,
        7 | 14 => sdl2::pixels::Color::YELLOW,
        _ => sdl2::pixels::Color::CYAN,
    }
}

fn read_screen_state(cpu: &mut CPU, frame: &mut [u8; 32 * 3 * 32]) -> bool {
    let mut frame_idx = 0;
    let mut update = false;
    for i in 0x0200..0x600 {
        let color_idx = cpu.mem_read(i as u16);
        let (b1, b2, b3) = color(color_idx).rgb();
        if frame[frame_idx] != b1 || frame[frame_idx + 1] != b2 || frame[frame_idx + 2] != b3 {
            frame[frame_idx] = b1;
            frame[frame_idx + 1] = b2;
            frame[frame_idx + 2] = b3;
            update = true;
        }
        frame_idx += 3;
    }
    update
}

fn handle_user_input(cpu: &mut CPU, event_pump: &mut EventPump) {
    for event in event_pump.poll_iter() {
        match event {
            Event::Quit { .. }
            | Event::KeyDown {
                keycode: Some(Keycode::Escape),
                ..
            } => std::process::exit(0),
            Event::KeyDown {
                keycode: Some(Keycode::W),
                ..
            } => {
                cpu.mem_write(0xff, 0x77);
            }
            Event::KeyDown {
                keycode: Some(Keycode::S),
                ..
            } => {
                cpu.mem_write(0xff, 0x73);
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                ..
            } => {
                cpu.mem_write(0xff, 0x61);
            }
            Event::KeyDown {
                keycode: Some(Keycode::D),
                ..
            } => {
                cpu.mem_write(0xff, 0x64);
            }
            _ => { /* do nothing */ }
        }
    }
}


fn main() {
    // init sdl2
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let window = video_subsystem
        .window("Tile viewer", (256.0 * 3.0) as u32, (240.0 * 3.0) as u32)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().present_vsync().build().unwrap();
    let mut event_pump = sdl_context.event_pump().unwrap();
    canvas.set_scale(3.0, 3.0).unwrap();

    let creator = canvas.texture_creator();
    let mut texture = creator
        .create_texture_target(PixelFormatEnum::RGB24, 256, 240)
        .unwrap();

    //load the game
    let bytes: Vec<u8> = std::fs::read("Pac-Man (USA) (Tengen).nes").unwrap();
    let rom = Rom::new(&bytes).unwrap();

    let mut frame = Frame::new();

    // run the game cycle
    let mut key_map = HashMap::new();
   key_map.insert(Keycode::Down, controller::JoypadButton::DOWN);
   key_map.insert(Keycode::Up, controller::JoypadButton::UP);
   key_map.insert(Keycode::Right, controller::JoypadButton::RIGHT);
   key_map.insert(Keycode::Left, controller::JoypadButton::LEFT);
   key_map.insert(Keycode::Space, controller::JoypadButton::SELECT);
   key_map.insert(Keycode::Return, controller::JoypadButton::START);
   key_map.insert(Keycode::A, controller::JoypadButton::BUTTON_A);
   key_map.insert(Keycode::S, controller::JoypadButton::BUTTON_B);


   // run the game cycle
   let bus = Bus::new(rom, move |ppu: &MyPPU, joypad: &mut controller::Joypad| {
       graphics_data::render(ppu, &mut frame);
       texture.update(None, &frame.data, 256 * 3).unwrap();

       canvas.copy(&texture, None, None).unwrap();

       canvas.present();
       for event in event_pump.poll_iter() {
           match event {
               Event::Quit { .. }
               | Event::KeyDown {
                   keycode: Some(Keycode::Escape),
                   ..
               } => std::process::exit(0),


               Event::KeyDown { keycode, .. } => {
                   if let Some(key) = key_map.get(&keycode.unwrap_or(Keycode::Ampersand)) {
                       joypad.set_button_pressed_status(*key, true);
                   }
               }
               Event::KeyUp { keycode, .. } => {
                   if let Some(key) = key_map.get(&keycode.unwrap_or(Keycode::Ampersand)) {
                       joypad.set_button_pressed_status(*key, false);
                   }
               }

               _ => { /* do nothing */ }
           }
       }
   });

    let mut cpu = CPU::new(bus);

    cpu.reset();
    cpu.run();
}  