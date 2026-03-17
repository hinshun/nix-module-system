# Simple standalone example that works with evalModules.
# This module does not require imports or pkgs.

{ config, lib, ... }:

{
  options = {
    greeting = lib.mkOption {
      type = lib.types.str;
      default = "Hello";
      description = "The greeting message";
    };

    name = lib.mkOption {
      type = lib.types.str;
      default = "World";
      description = "Who to greet";
    };

    count = lib.mkOption {
      type = lib.types.int;
      default = 1;
      description = "How many times to greet";
    };

    enabled = lib.mkEnableOption "greeting feature";
  };

  config = {
    greeting = lib.mkDefault "Hi";
    enabled = true;
  };
}
