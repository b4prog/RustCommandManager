use rcm::viewport::Viewport;

#[test]
fn scrolling_retains_only_recent_lines_and_clears_them_at_the_end() {
	let mut viewport = Viewport::new(3, 40);
	let mut output = Vec::from(b"Header\r\n".as_slice());
	viewport
		.consume(b"one\r\ntwo\r\nthree\r\nfour\r\nfive", &mut output)
		.unwrap();
	let mut terminal = vt100::Parser::new(20, 40, 0);
	terminal.process(&output);
	assert_eq!(terminal.screen().contents(), "Header\nthree\nfour\nfive");
	let mut cleared = Vec::new();
	viewport.clear(&mut cleared).unwrap();
	terminal.process(&cleared);
	assert_eq!(terminal.screen().contents(), "Header");
	terminal.process(b"Next command\r\n");
	assert_eq!(terminal.screen().contents(), "Header\nNext command");
}

#[test]
fn colors_survive_scrolling_and_escape_sequences_split_between_chunks() {
	let mut viewport = Viewport::new(2, 40);
	let mut output = Vec::new();
	viewport.consume(b"\x1b[3", &mut output).unwrap();
	viewport
		.consume(b"1mold\r\nsecond\r\nred", &mut output)
		.unwrap();
	let mut terminal = vt100::Parser::new(20, 40, 0);
	terminal.process(&output);
	assert_eq!(terminal.screen().contents(), "second\nred");
	assert_eq!(
		terminal.screen().cell(0, 0).unwrap().fgcolor(),
		vt100::Color::Idx(1)
	);
	assert_eq!(
		terminal.screen().cell(1, 0).unwrap().fgcolor(),
		vt100::Color::Idx(1)
	);
}

#[test]
fn carriage_returns_replace_progress_without_erasing_other_rows() {
	let mut viewport = Viewport::new(3, 40);
	let mut output = Vec::new();
	viewport
		.consume(b"Completed\r\nProgress: 10%\rProgress: 90%", &mut output)
		.unwrap();
	let mut terminal = vt100::Parser::new(20, 40, 0);
	terminal.process(&output);
	assert_eq!(terminal.screen().contents(), "Completed\nProgress: 90%");
}

#[test]
fn unicode_and_long_lines_stay_inside_the_reserved_rows() {
	let mut viewport = Viewport::new(2, 8);
	let mut output = Vec::from(b"Header\r\n".as_slice());
	viewport
		.consume("abcdefgh12345678é界".as_bytes(), &mut output)
		.unwrap();
	let mut terminal = vt100::Parser::new(20, 8, 0);
	terminal.process(&output);
	assert_eq!(terminal.screen().contents(), "Header\n12345678\né界");
	let mut clear = Vec::new();
	viewport.clear(&mut clear).unwrap();
	terminal.process(&clear);
	assert_eq!(terminal.screen().contents(), "Header");
}

#[test]
fn resizing_clears_the_old_viewport_and_allocates_the_new_height() {
	let mut viewport = Viewport::new(3, 40);
	let mut output = Vec::new();
	viewport
		.consume(b"one\r\ntwo\r\nthree", &mut output)
		.unwrap();
	viewport.resize(2, 40, &mut output).unwrap();
	viewport.consume(b"\r\nfour", &mut output).unwrap();
	let mut terminal = vt100::Parser::new(20, 40, 0);
	terminal.process(&output);
	assert_eq!(terminal.screen().contents(), "three\nfour");
}

#[test]
fn clearing_without_output_does_not_write_terminal_controls() {
	let mut viewport = Viewport::new(5, 80);
	let mut output = Vec::new();
	viewport.clear(&mut output).unwrap();
	assert!(output.is_empty());
}
