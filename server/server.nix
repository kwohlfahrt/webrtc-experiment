{ stdenv, makeRustPlatform, rustChannels, openssl, gst_all_1, pkgconfig, libnice }:

let
  rustPlatform = with rustChannels.stable; makeRustPlatform {
    inherit cargo;
    rustc = rust;
  };
in rustPlatform.buildRustPackage rec {
  pname = "pimostat";
  version = "0.1.0";

  src = ./.;
  nativeBuildInputs = [ openssl pkgconfig libnice ] ++ (with gst_all_1;
    [ gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad ]
  );

  # stable
  #cargoSha256 = "1c3nq9m7wskvz8g0cvx02ljgzn7g35vjxkr0z3xdnci237sf1jad";
  # unstable
  cargoSha256 = "1h6v2lphyqfp12js31mj1hfwhy85d4r28s2jkl5lgvfa5bfpa9s5";

  meta = with stdenv.lib; {
    platforms = platforms.all;
  };
}
