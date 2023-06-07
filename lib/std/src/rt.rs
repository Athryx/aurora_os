#[lang = "start"]
fn lang_start<T: 'static>(main: fn() -> T, _argc: isize, _argv: *const *const u8, _sigpipe: u8) -> isize {
    main();

    loop { core::hint::spin_loop(); }
}