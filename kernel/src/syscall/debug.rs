use crate::prelude::*;
use crate::io::R_WRITER;

/// Prints the characters specified in the arguments to the debug console
/// 
/// this syscall is only for debugging until I write a terminal emulator
/// each argument is a combination of 8 bit characters to print to the screen
/// the order the characters are printed is as follows:
/// lower number arguments are printed before higher numbered arguments (a1 before a2 before a3, etc)
/// least significant bytes in each argument are printed first (a1 bits 0-7, a1 bits 8-15, a1 bits 16-23, etc)
///
/// # Options
/// bits 0-7 (debug_print_num): specifies the number of characters to print (max 80 on x86_64)
pub fn print_debug(
    options: u32,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
    a8: usize,
    a9: usize,
    a10: usize,
) -> KResult<()> {
    fn print_bytes(bytes: usize, mut n: usize) -> usize
	{
		let mut i = 0;
		while i < core::mem::size_of::<usize>() && n > 0 {
			unsafe {
				R_WRITER.write_byte(get_bits(bytes, (8 * i)..(8 * i + 8)) as u8);
			}
			i += 1;
			n -= 1;
		}
		n
	}

	let mut n = core::cmp::min(options, 80) as usize;
	n = print_bytes(a1, n);
	n = print_bytes(a2, n);
	n = print_bytes(a3, n);
	n = print_bytes(a4, n);
	n = print_bytes(a5, n);
	n = print_bytes(a6, n);
	n = print_bytes(a7, n);
	n = print_bytes(a8, n);
	n = print_bytes(a9, n);
	print_bytes(a10, n);

    Ok(())
}
