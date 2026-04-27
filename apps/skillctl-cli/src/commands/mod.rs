//! 子命令模块。
//!
//! 每个子命令保持一个独立 module，便于后续填充 + 测试隔离。
//! 当前阶段仅承载 clap Args 与未实现 stub。

use skillctl_core::Result;

use super::{Command, OutputFormat};

pub mod util;

pub mod add;
pub mod describe;
pub mod disable;
pub mod doctor;
pub mod enable;
pub mod init;
pub mod inspect;
pub mod list;
pub mod path;
pub mod profile;
pub mod remove;
pub mod restore;
pub mod show;
pub mod trust;
pub mod update;
pub mod use_cmd;
pub mod validate;
pub mod verify;

pub fn dispatch(cmd: Command, fmt: OutputFormat) -> Result<()> {
    match cmd {
        Command::Init(a) => init::run(a, fmt),
        Command::Profile(c) => profile::run(c, fmt),
        Command::Add(a) => add::run(a, fmt),
        Command::Remove(a) => remove::run(a, fmt),
        Command::Use(a) => use_cmd::run(a, fmt),
        Command::Enable(a) => enable::run(a, fmt),
        Command::Disable(a) => disable::run(a, fmt),
        Command::List(a) => list::run(a, fmt),
        Command::Describe(a) => describe::run(a, fmt),
        Command::Show(a) => show::run(a, fmt),
        Command::Path(a) => path::run(a, fmt),
        Command::Restore(a) => restore::run(a, fmt),
        Command::Update(a) => update::run(a, fmt),
        Command::Doctor(a) => doctor::run(a, fmt),
        Command::Validate(a) => validate::run(a, fmt),
        Command::Verify(a) => verify::run(a, fmt),
        Command::Inspect(a) => inspect::run(a, fmt),
        Command::Trust(a) => trust::run(a, fmt),
    }
}
