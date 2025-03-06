extern crate lvm_sys2;
extern crate rstest;

use lvm_sys2::lvm::Lvm;
use rstest::rstest;

/// this test doesn't require any
#[test]
fn basic_init_deinit() {
    Lvm::acquire_and(|_lvm| Ok("some".to_string())).expect("lock or init failed");
}

#[test]
#[should_panic]
#[ignore = "this test poisons global LVM handler"]
fn poison() {
    Lvm::acquire_and::<(), _>(|_lvm| panic!("here we poison the lock"))
        .expect("lock or init failed");
}

// test sample RO command
// Pre-requirements: see README.md
#[rstest]
// TODO: does it run in parallel?
#[case("pvs")]
#[case("vgs")]
#[case("lvs")]
fn test_run_ro_command(#[case] cmd: &str) {
    // pvcreate
    let _res = Lvm::run(cmd).unwrap();
    // pvremove
}
