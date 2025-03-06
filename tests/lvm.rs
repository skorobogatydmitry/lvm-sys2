extern crate lvm_sys2;
use lvm_sys2::lvm::Lvm;

/// this test doesn't require any
#[test]
fn basic_init_deinit() {
    Lvm::acquire_and(|_lvm| Ok("some".to_string())).expect("lock or init failed");
}

#[test]
#[should_panic]
#[ignore = "this test poisons global LVM handler"]
fn poison() {
    Lvm::acquire_and(|_lvm| panic!("here we poison the lock")).expect("lock or init failed");
}

// test sample RO command
// Pre-requirements: see README.md
#[test]
fn test_run_ro_command() {
    // pvcreate
    let _res = Lvm::run("pvs").unwrap();
    // pvremove
}
