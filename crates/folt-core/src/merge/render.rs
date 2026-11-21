/// Accumulates bytes and the exact conflict marker lines written into them.
#[derive(Default)]
pub(super) struct Render {
    pub bytes: Vec<u8>,
    pub marker_lines: Vec<Vec<u8>>,
}

impl Render {
    pub fn conflict(&mut self, ours: &[u8], theirs: &[u8]) {
        self.marker(b"<<<<<<<\n");
        self.bytes.extend_from_slice(ours);
        self.marker(b"=======\n");
        self.bytes.extend_from_slice(theirs);
        self.marker(b">>>>>>>\n");
    }

    fn marker(&mut self, line: &[u8]) {
        // An unterminated input line must not swallow a conflict marker line.
        if !self.bytes.is_empty() && !self.bytes.ends_with(b"\n") {
            self.bytes.push(b'\n');
        }
        self.bytes.extend_from_slice(line);
        self.marker_lines.push(line.to_vec());
    }
}
