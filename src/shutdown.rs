use core::cell::OnceCell;

use alloc::boxed::Box;
use critical_section::Mutex;
use device_tree_parser::{DeviceTreeNode, DtbError};
use qemu_exit::QEMUExit;

use crate::println;

static GLOBAL_SHUTDOWN: Mutex<OnceCell<&'static dyn Shutdown>> = Mutex::new(OnceCell::new());

#[derive(Debug, Clone, thiserror::Error)]
pub enum InitError<'a> {
    #[error("dtb error: {0}")]
    DtbError(DtbError),
    #[error("missing reg property for device {0}")]
    NoReg(&'a str),
    #[error("shutdown::init called more than once")]
    AlreadyInitialized,
}

pub fn init<'a>(dt_root: &DeviceTreeNode<'a>) -> Result<(), InitError<'a>> {
    const COMPATIBLE: &str = "sifive,test1";

    let shutdown = {
        let test_devices = dt_root.find_compatible_nodes(COMPATIBLE);
        if let Some(test_dev) = test_devices.first() {
            let reg = test_dev
                .translate_reg_addresses(Some(dt_root))
                .map_err(InitError::DtbError)?;
            let (addr, _) = reg
                .first()
                .copied()
                .ok_or(InitError::NoReg(test_dev.name))?;
            println!(
                "found sifive_test device {} at 0x{addr:016x}",
                test_dev.name
            );
            Box::leak(Box::new(QemuShutdown::new(addr))) as _
        } else {
            &SBI_SHUTDOWN_SINGLETON as _
        }
    };

    critical_section::with(|cs| GLOBAL_SHUTDOWN.borrow(cs).set(shutdown))
        .map_err(|_| InitError::AlreadyInitialized)
}

pub fn get() -> &'static dyn Shutdown {
    critical_section::with(|cs| {
        *GLOBAL_SHUTDOWN
            .borrow(cs)
            .get_or_init(|| &SBI_SHUTDOWN_SINGLETON)
    })
}

pub trait Shutdown: Send + Sync {
    #[cfg(test)]
    fn shutdown_success(&self) -> !;
    fn shutdown_failure(&self) -> !;
}

pub struct SbiShutdown;

static SBI_SHUTDOWN_SINGLETON: SbiShutdown = SbiShutdown;

impl Shutdown for SbiShutdown {
    #[cfg(test)]
    fn shutdown_success(&self) -> ! {
        _ = sbi_rt::system_reset(sbi_rt::Shutdown, sbi_rt::NoReason);
        never_return()
    }

    fn shutdown_failure(&self) -> ! {
        _ = sbi_rt::system_reset(sbi_rt::Shutdown, sbi_rt::SystemFailure);
        never_return()
    }
}

fn never_return() -> ! {
    loop {
        riscv::asm::wfi();
    }
}

pub struct QemuShutdown {
    addr: u64,
}

impl QemuShutdown {
    fn new(addr: u64) -> Self {
        Self { addr }
    }

    fn qemu_exit(&self) -> qemu_exit::RISCV64 {
        unsafe { qemu_exit::RISCV64::new(self.addr) }
    }
}

impl Shutdown for QemuShutdown {
    #[cfg(test)]
    fn shutdown_success(&self) -> ! {
        self.qemu_exit().exit_success()
    }

    fn shutdown_failure(&self) -> ! {
        self.qemu_exit().exit_failure()
    }
}
