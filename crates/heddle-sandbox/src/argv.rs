//! Turning an argument **vector** into the single command-line **string**
//! `CreateProcessW` insists on.
//!
//! There is no shell anywhere in this crate and no argument is ever
//! interpreted, but `CreateProcessW` takes one string, so the vector has to
//! survive a round trip through the MSVCRT quoting rules. The unit tests below
//! use the **real** `CommandLineToArgvW` as their oracle rather than a
//! hand-written mirror of those rules, because a mirror would agree with a
//! wrong builder.
//!
//! **What this discipline does and does not buy.** It removes any possibility
//! of one argument becoming two, or of a quote inside an argument ending it. It
//! does **not** stop a model naming `cmd.exe` as the command — nothing here
//! could, and a blocklist of shell binaries would be theatre. The containment
//! boundary is the AppContainer, the Job Object and the human approval, not the
//! identity of the executable.

use crate::ARG_COUNT_CAP;

/// `CreateProcessW`'s documented maximum for `lpCommandLine`, minus room for
/// the NUL. A refusal naming the limit is something the model can act on; the
/// `ERROR_INVALID_PARAMETER` it would otherwise meet is not.
const COMMAND_LINE_LIMIT: usize = 32_000;

/// `exe` first, then each argument quoted only as much as it needs.
///
/// Every refusal here happens **before** any process exists, and each names
/// what it refused so the model receives something it can retry differently.
pub(crate) fn command_line(exe: &str, args: &[String]) -> Result<Vec<u16>, String> {
    if args.len() > ARG_COUNT_CAP {
        return Err(format!(
            "{} arguments is over the {ARG_COUNT_CAP}-argument cap; pass fewer",
            args.len()
        ));
    }
    // A NUL would silently truncate the command line at the point it appears,
    // which is a wrong answer in a right answer's shape — the process would run
    // with fewer arguments than the model asked for and nothing would say so.
    if let Some(bad) = args.iter().position(|arg| arg.contains('\0')) {
        return Err(format!(
            "argument {bad} contains a NUL byte, which no command line can carry"
        ));
    }

    let mut line = String::new();
    quote_into(&mut line, exe);
    for arg in args {
        line.push(' ');
        quote_into(&mut line, arg);
    }

    let wide: Vec<u16> = line.encode_utf16().chain(std::iter::once(0)).collect();
    if wide.len() > COMMAND_LINE_LIMIT {
        return Err(format!(
            "the command line is {} UTF-16 units, over the {COMMAND_LINE_LIMIT}-unit limit; pass \
             shorter arguments",
            wide.len()
        ));
    }
    Ok(wide)
}

/// One argument, quoted per the MSVCRT rules `CommandLineToArgvW` parses.
///
/// A backslash is only special immediately before a quote — including the
/// closing quote this function adds — which is why the run length is counted
/// rather than every backslash being doubled. The empty string needs quotes or
/// it would vanish entirely.
fn quote_into(line: &mut String, arg: &str) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        line.push_str(arg);
        return;
    }
    line.push('"');
    let mut backslashes = 0usize;
    for character in arg.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                // 2n+1: the doubled run, then one escaping this quote.
                line.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                line.push('"');
                backslashes = 0;
            }
            _ => {
                line.extend(std::iter::repeat_n('\\', backslashes));
                line.push(character);
                backslashes = 0;
            }
        }
    }
    line.extend(std::iter::repeat_n('\\', backslashes * 2));
    line.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::UI::Shell::CommandLineToArgvW;

    /// The real Win32 parser, as the oracle. A hand-written mirror of the
    /// quoting rules would agree with a wrong builder; this cannot.
    fn parse(line: &[u16]) -> Vec<String> {
        let mut count = 0i32;
        unsafe {
            let argv = CommandLineToArgvW(PCWSTR(line.as_ptr()), &mut count);
            assert!(!argv.is_null(), "the command line must parse: {count}");
            let parsed = (0..count as isize)
                .map(|i| {
                    (*argv.offset(i))
                        .to_string()
                        .expect("an argument is valid UTF-16")
                })
                .collect();
            let _ = LocalFree(Some(std::mem::transmute::<
                *mut windows::core::PWSTR,
                windows::Win32::Foundation::HLOCAL,
            >(argv)));
            parsed
        }
    }

    #[test]
    fn every_adversarial_argument_survives_the_round_trip() {
        let exe = r"C:\Windows\System32\cmd.exe";
        let args: Vec<String> = [
            "plain",
            "with space",
            "with\ttab",
            r#"a"b"#,
            r"a\",
            r"a\\",
            r#"trailing\"#,
            r#""quoted whole""#,
            "",
            "&",
            "|",
            ">",
            "<",
            "&&",
            "a b\"c\\",
            r"C:\path with space\file.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let line = command_line(exe, &args).expect("the command line builds");

        let mut expected = vec![exe.to_string()];
        expected.extend(args.clone());
        assert_eq!(
            parse(&line),
            expected,
            "built: {}",
            String::from_utf16_lossy(&line)
        );
    }

    #[test]
    fn too_many_arguments_is_a_named_refusal() {
        let args: Vec<String> = (0..=ARG_COUNT_CAP).map(|i| i.to_string()).collect();

        let refusal = command_line("cmd.exe", &args).expect_err("65 arguments must be refused");

        assert!(
            refusal.contains("65 arguments") && refusal.contains(&ARG_COUNT_CAP.to_string()),
            "the refusal must name both numbers: {refusal}"
        );
    }

    #[test]
    fn an_embedded_nul_is_a_named_refusal() {
        let args = vec!["fine".to_string(), "bad\0tail".to_string()];

        let refusal = command_line("cmd.exe", &args).expect_err("a NUL must be refused");

        assert!(
            refusal.contains("argument 1") && refusal.contains("NUL"),
            "the refusal must say which argument and why: {refusal}"
        );
    }

    #[test]
    fn an_oversized_command_line_is_a_named_refusal() {
        // Under the argument cap, over the length limit: the two refusals are
        // independent and this proves the second is reachable at all.
        let args = vec!["x".repeat(COMMAND_LINE_LIMIT + 1)];

        let refusal =
            command_line("cmd.exe", &args).expect_err("an oversized command line must be refused");

        assert!(
            refusal.contains("UTF-16 units") && refusal.contains(&COMMAND_LINE_LIMIT.to_string()),
            "the refusal must name the limit: {refusal}"
        );
    }
}
