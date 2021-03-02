{ lib, buildPythonApplication, build, websockets, pygobject3, gst-python, gobject-introspection, wrapGAppsHook, gst_all_1, libnice }:

buildPythonApplication rec {
  pname = "webrtc";
  version = "0.1.0";

  strictDeps = false;

  src = ./.;
  nativeBuildInputs = [ wrapGAppsHook build ];
  buildInputs = [ libnice ] ++ (with gst_all_1;
    [ gst-plugins-base gst-plugins-good gst-plugins-bad ]
  );
  propagatedBuildInputs = [
    gobject-introspection
    pygobject3
    gst-python
    websockets
  ];

  meta = with lib; {
    platforms = platforms.all;
  };
}
