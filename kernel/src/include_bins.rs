pub fn get_binaries() -> alloc::vec::Vec<&'static [u8]>{
	alloc::vec![
		include_bytes!("/home/louismeller/Coding/os/tinyOS/kernel/../tinyosprograms/programs/example-asm/a.out"),
		include_bytes!("/home/louismeller/Coding/os/tinyOS/kernel/../tinyosprograms/programs/example-rs/a.out"),
	]
}
