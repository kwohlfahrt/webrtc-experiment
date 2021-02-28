use webrtc::signalling;

use clap::{App, Arg};

#[actix_web::main]
async fn main() -> Result<(), signalling::Error> {
    let matches = App::new("Signalling server")
        .arg(Arg::with_name("address"))
        .get_matches();

    let address = matches.value_of("address").unwrap_or("localhost:4000");

    signalling::main(address).await
}
