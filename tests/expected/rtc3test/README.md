Button scripts (`gb-cli run --input`) that pick the rtc3test sub-test from its menu and wait long
enough for it to finish (A = basic, DOWN,A = range, DOWN,DOWN,A = sub-second writes). Compared
against `rtc3test-*-dmg.png` shipped with the test ROM; see `crates/gb-cli/tests/acid2.rs`.
