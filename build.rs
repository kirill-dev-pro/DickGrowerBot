use include_dir::{include_dir, Dir};

static LOCALES: Dir = include_dir!("$CARGO_MANIFEST_DIR/locales");

fn main() {
    for file in LOCALES.files() {
        println!("{}: {} bytes", file.path().display(), file.contents().len());
    }
}
