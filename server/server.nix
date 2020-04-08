{ stdenv, makeRustPlatform, rustChannels, openssl, gst_all_1, pkgconfig }:

let
  rustPlatform = with rustChannels.stable; makeRustPlatform {
    inherit cargo;
    rustc = rust;
  };
in rustPlatform.buildRustPackage rec {
  pname = "pimostat";
  version = "0.1.0";

  src = ./.;
  nativeBuildInputs = [ openssl pkgconfig ] ++ (with gst_all_1;
    [ gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad ]
  );

  cargoSha256 = "1c3nq9m7wskvz8g0cvx02ljgzn7g35vjxkr0z3xdnci237sf1jad";

  meta = with stdenv.lib; {
    platforms = platforms.all;
  };
}
