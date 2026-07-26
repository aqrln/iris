use crate::{print, println, shutdown};

pub fn test_runner(tests: &[&Test]) {
    println!("running {} kernel tests", tests.len());
    for test in tests {
        print!("test {} ...", test.name);
        (test.run)();
        println!(" ok")
    }
    shutdown::get().shutdown_success();
}

pub struct Test {
    name: &'static str,
    run: fn(),
}

macro_rules! test {
    () => {};

    ($name:ident $body:block $($t:tt)*) => {
        fn $name() $body
        ::paste::paste! {
            #[cfg(test)]
            #[test_case]
            #[allow(non_upper_case_globals)]
            static [<_TEST_ $name>]: $crate::test::Test = $crate::test::Test {
                name: ::core::any::type_name_of_val(&$name),
                run: $name,
            };
        }

        test!($($t)*);
    };
}

test! {
    test_true {
        assert!(true);
    }

    test_one_is_one {
        assert_eq!(1, 1);
    }
}
