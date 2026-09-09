use windows::Win32::System::Power::{CallNtPowerInformation, SystemExecutionState};
fn main() {
    unsafe {
        let mut state: u32 = 0;
        let res = CallNtPowerInformation(
            SystemExecutionState,
            None,
            Some(&mut state as *mut _ as *mut _),
            4,
        );
        println!("Res: {:?}, state: {:x}", res, state);
    }
}
