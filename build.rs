use vergen::vergen;

fn main() {
    let mut config = vergen::Config::default();
    *config.git_mut().sha_kind_mut() = vergen::ShaKind::Short;
    vergen(config).unwrap();
}
