{ mkYarnPackage }:

mkYarnPackage rec {
  name = "webrtc";
  version = "0.1.0";
  src = ./.;
  packageJSON = ./package.json;

  buildPhase = ''
    yarn --offline --cache-folder /build/.yarn-cache build --mode=production
  '';
  installPhase = ''
    mv deps/${name}-client/dist $out
  '';
  distPhase = "true";
}
