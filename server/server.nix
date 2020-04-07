{ stdenv, makeRustPlatform, rustChannels, openssl, pkgconfig, bash }:

let
  rustPlatform = with rustChannels.stable; makeRustPlatform {
    inherit cargo;
    rustc = rust;
  };
in rustPlatform.buildRustPackage rec {
  pname = "pimostat";
  version = "0.1.0";

  src = ./.;
  nativeBuildInputs = [ openssl pkgconfig ];

  cargoSha256 = "0jis4h9agri19sz6sv174hml0a7wy8l444rsj26g6vwfh22d6x93";

  meta = with stdenv.lib; {
    platforms = platforms.all;
  };
}
