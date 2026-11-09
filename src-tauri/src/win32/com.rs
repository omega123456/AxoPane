use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT};

pub struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    pub fn initialize(model: COINIT) -> Self {
        Self {
            initialized: unsafe { CoInitializeEx(None, model).is_ok() },
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}
