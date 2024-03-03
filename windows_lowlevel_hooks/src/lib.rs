use lazy_static::lazy_static;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageA, GetMessageA, HHOOK, KBDLLHOOKSTRUCT, MSG, WM_KEYDOWN,
};

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc, RwLock};
use std::thread::{self, JoinHandle};

use windows::{
    core::IUnknown,
    Win32::{
        Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{self, WH_KEYBOARD_LL},
    },
};

#[derive(Debug)]
pub enum HookMessage {
    Key {
        direction: Direction,
        sys: bool,
        data: KBDLLHOOKSTRUCT,
    },
}

#[derive(Debug)]
pub enum Direction {
    Up,
    Down,
}

lazy_static! {
    pub static ref KEY_SENDER: RwLock<Option<Sender<HookMessage>>> = { RwLock::new(None) };
}

pub fn start_thread() -> JoinHandle<()> {
    thread::spawn(move || loop {
        let hook_id = unsafe {
            WindowsAndMessaging::SetWindowsHookExA(
                WH_KEYBOARD_LL,
                Some(hook_callback),
                HINSTANCE::default(),
                0,
            )
            .unwrap()
        };

        let mut message = MSG::default();
        // Start pumping messages
        unsafe {
            while GetMessageA(&mut message, None, 0, 0).into() {
                DispatchMessageA(&message);
            }
        }

        unsafe {
            let _ = WindowsAndMessaging::UnhookWindowsHookEx(hook_id);
        }
    })
}

extern "system" fn hook_callback(code: i32, wParam: WPARAM, lParam: LPARAM) -> LRESULT {
    match KEY_SENDER.read() {
        Ok(sender) => match sender.as_ref() {
            Some(sender) => {
                let kb = unsafe {
                    let kb = lParam.0 as *const KBDLLHOOKSTRUCT;
                    *kb.clone()
                };
                let msg = match (wParam.0 as u32) {
                    WM_KEYDOWN => HookMessage::Key {
                        direction: Direction::Down,
                        sys: false,
                        data: kb,
                    },
                    WM_KEYUP => HookMessage::Key {
                        direction: Direction::Up,
                        sys: false,
                        data: kb,
                    },
                    WM_SYSKEYDOWN => HookMessage::Key {
                        direction: Direction::Down,
                        sys: true,
                        data: kb,
                    },
                    WM_SYSKEYUP => HookMessage::Key {
                        direction: Direction::Up,
                        sys: true,
                        data: kb,
                    },
                };
                let _ = sender.send(msg);
            }
            None => (),
        },
        _ => (),
    }
    //LRESULT(1) // only do this if i want to block keys!
    unsafe { CallNextHookEx(HHOOK::default(), code, wParam, lParam) }
}
