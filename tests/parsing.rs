#[cfg(feature = "client")]
#[test]
fn parse_examples() {
    for file in std::fs::read_dir("./tests/examples").unwrap() {
        println!("{:?}", file);
        let contents = std::fs::read_to_string(file.unwrap().path()).unwrap();
        rsst::client::RssResponse::from_string(contents).unwrap();
    }
}
