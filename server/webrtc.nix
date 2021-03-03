{ lib, makeRustPlatform, rustChannels, gst_all_1, pkgconfig, libnice, glib }:

let
  rustPlatform = with rustChannels.stable; makeRustPlatform {
    inherit cargo;
    rustc = rust;
  };
in rustPlatform.buildRustPackage rec {
  pname = "webrtc";
  version = "0.1.0";

  src = ./.;
  nativeBuildInputs = [ pkgconfig ];
  buildInputs = [ libnice glib.dev ] ++ (with gst_all_1;
    [ gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad ]
  );

  cargoSha256 = "0qz5ipqw6kpk6c241jkamy9h9pcy1gp80av9rcddgjcwin0s8926";

  meta = with lib; {
    platforms = platforms.all;
  };
}
