macro_rules! test_recording {
    ($name:ident, $path:expr, $cable:expr, $cable_estimated:expr) => {
        #[test]
        #[tracing_test::traced_test]
        fn $name() {
            use std::io::Cursor;

            use crate::commands::file::extract_recoveries;
            use crate::track::{Grading, TrackResult};

            let acmi = include_bytes!($path);
            let recoveries = extract_recoveries(&mut Cursor::new(acmi)).unwrap();
            let [recovery]: [TrackResult; 1] = recoveries.try_into().unwrap();
            assert_eq!(
                recovery.grading,
                Grading::Recovered {
                    cable: Some($cable),
                    cable_estimated: Some($cable_estimated)
                }
            );
        }
    };
}

test_recording!(wire_1_01, "../tests/recordings/wire_1_01.zip.acmi", 1, 1);
test_recording!(wire_2_01, "../tests/recordings/wire_2_01.zip.acmi", 2, 2);
test_recording!(wire_4_01, "../tests/recordings/wire_4_01.zip.acmi", 4, 4);
