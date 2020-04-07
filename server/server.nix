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

  cargoSha256 = "0b1x6z2ngqka0zqqpnnks04rz6b8pwjchbdlamcac535q6nsmc7v";

  meta = with stdenv.lib; {
    platforms = platforms.all;
  };
}
