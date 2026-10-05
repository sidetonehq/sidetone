use crate::{guard, sys};
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Begin,
    Continue,
    End,
}

/// A reference to an X-Plane command (ours or the sim's).
#[derive(Clone, Copy, Debug)]
pub struct Command(sys::XPLMCommandRef);

impl Command {
    pub fn find(name: &str) -> Option<Command> {
        let c = CString::new(name).ok()?;
        let r = unsafe { sys::XPLMFindCommand(c.as_ptr()) };
        (!r.is_null()).then_some(Command(r))
    }

    /// Creates the command, or returns the existing one with this name.
    pub fn create(name: &str, description: &str) -> Option<Command> {
        let n = CString::new(name).ok()?;
        let d = CString::new(description).ok()?;
        let r = unsafe { sys::XPLMCreateCommand(n.as_ptr(), d.as_ptr()) };
        (!r.is_null()).then_some(Command(r))
    }

    pub fn once(&self) {
        unsafe { sys::XPLMCommandOnce(self.0) }
    }
}

type Handler = Box<dyn FnMut(Phase) -> bool>;

struct Registration {
    command: Command,
    before: bool,
    handler: Handler,
}

/// Keeps a command handler alive; unregisters on drop.
pub struct CommandHandler(Box<Registration>);

impl CommandHandler {
    /// Registers `handler`. It returns `true` to let X-Plane (and other handlers) keep
    /// processing the command, `false` to consume it.
    pub fn register(command: Command, before: bool, handler: impl FnMut(Phase) -> bool + 'static) -> CommandHandler {
        let mut reg = Box::new(Registration { command, before, handler: Box::new(handler) });
        unsafe {
            sys::XPLMRegisterCommandHandler(command.0, Some(trampoline), before as c_int, &mut *reg as *mut Registration as *mut c_void);
        }
        CommandHandler(reg)
    }
}

impl Drop for CommandHandler {
    fn drop(&mut self) {
        let reg = &mut *self.0;
        unsafe {
            sys::XPLMUnregisterCommandHandler(reg.command.0, Some(trampoline), reg.before as c_int, reg as *mut Registration as *mut c_void);
        }
    }
}

unsafe extern "C" fn trampoline(_cmd: sys::XPLMCommandRef, phase: sys::XPLMCommandPhase, refcon: *mut c_void) -> c_int {
    guard("command handler", 1, || {
        let reg = unsafe { &mut *(refcon as *mut Registration) };
        let phase = match phase as u32 {
            sys::xplm_CommandBegin => Phase::Begin,
            sys::xplm_CommandContinue => Phase::Continue,
            _ => Phase::End,
        };
        (reg.handler)(phase) as c_int
    })
}
