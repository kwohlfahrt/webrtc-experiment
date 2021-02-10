{ nixpkgs ? import <nixpkgs> {} }: with nixpkgs; callPackage ./webrtc.nix {}
