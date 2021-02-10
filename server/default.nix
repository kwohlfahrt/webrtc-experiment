{ nixpkgs ? import <nixpkgs> {} }: with nixpkgs; callPackage ./webrtc.nix {} // {
  # Environment Variables
  RUST_BACKTRACE = 1;
}
