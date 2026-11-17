use super::*;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
static VM_RESULT: AtomicI32=AtomicI32::new(1);
static VM_QUERIES: AtomicUsize=AtomicUsize::new(0);
#[export_name="wsx_direct_bootstrap_vm_exited"]
extern "C" fn fake_vm_exit(_: *mut c_void) -> i32 {
    VM_QUERIES.fetch_add(1,Ordering::SeqCst);VM_RESULT.load(Ordering::SeqCst)
}
fn poll(mut f:impl FnMut()->io::Result<bool>) {
    let end=Instant::now()+Duration::from_secs(5);
    while !f().unwrap() {assert!(Instant::now()<end);std::thread::sleep(Duration::from_millis(20));}
}
#[test]
fn prebound_normal_external_and_vm_loss_never_respawn() {
    assert!(std::env::var_os("WSX_TEST_WSL").is_some(),"Explicit fake helper is required; never use real WSL in this fixture");
    let directory=std::env::temp_dir().join(format!("wsx-helper-test-{}",std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    // One serialized test owns these fixture environment settings.
    for scenario in ["normal","external","vm_before_request","vm_after_helper_error","helper_error"] {
        VM_RESULT.store(1,Ordering::SeqCst);
        let launches=directory.join(format!("{scenario}.launches"));
        std::env::set_var("WSX_TEST_LAUNCH_LOG",&launches);
        std::env::set_var("WSX_TEST_HELPER_FAIL",if scenario.contains("error") {"1"} else {"0"});
        let log=File::create(directory.join(format!("{scenario}.log"))).unwrap();
        let mut guest=unsafe {GuestRetirement::new("fixture-distro".into(),
            "12345678-1234-1234-1234-123456789abc".into(),log,1usize as *mut c_void,false)}.unwrap();
        poll(||guest.system_ready());
        assert_eq!(std::fs::read_to_string(&launches).unwrap().lines().count(),2);
        // Even if fallback discovery is pending, closing may only use existing
        // helpers. This fixture never launches an actual WSL list command.
        guest.next_query=Instant::now()+Duration::from_secs(30);
        if scenario=="external" {guest.distro_stopped=true;}
        if scenario=="vm_before_request" {VM_RESULT.store(0,Ordering::SeqCst);guest.next_vm_query=Instant::now();}
        guest.begin().unwrap();guest.begin().unwrap();
        if scenario=="vm_after_helper_error" || scenario=="helper_error" {
            let end=Instant::now()+Duration::from_secs(5);
            while guest.child.as_mut().unwrap().try_wait().unwrap().is_none() {
                assert!(Instant::now()<end);std::thread::sleep(Duration::from_millis(20));
            }
            assert!(!guest.confirmed().unwrap());
            if scenario=="vm_after_helper_error" {VM_RESULT.store(0,Ordering::SeqCst);guest.next_vm_query=Instant::now();}
            else {
                guest.helper_failure_since=Some(Instant::now()-Duration::from_secs(3));
                assert!(guest.confirmed().is_err());
            }
        }
        if scenario!="helper_error" {poll(||guest.confirmed());}
        let queries=VM_QUERIES.load(Ordering::SeqCst);
        guest.finish_helpers().unwrap();
        assert_eq!(queries,VM_QUERIES.load(Ordering::SeqCst));
        assert_eq!(std::fs::read_to_string(&launches).unwrap().lines().count(),2,"helper respawn: {scenario}");
        assert!(guest.system_child.is_none() && guest.normal_child.is_none());
        println!("PASS scenario={scenario}: exactly two pre-bound helpers, no replacement, natural exit");
    }
}
