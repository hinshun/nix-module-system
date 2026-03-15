# lib.nix - Module system library functions
# This provides mkOption, mkIf, types, and other helpers

rec {
  # Type definitions
  types = {
    bool = {
      _type = "type";
      name = "bool";
      check = x: builtins.isBool x;
      merge = defs: builtins.elemAt defs 0;
    };

    int = {
      _type = "type";
      name = "int";
      check = x: builtins.isInt x;
      merge = defs: builtins.elemAt defs 0;
    };

    str = {
      _type = "type";
      name = "str";
      check = x: builtins.isString x;
      merge = defs: builtins.concatStringsSep "\n" defs;
    };

    path = {
      _type = "type";
      name = "path";
      check = x: builtins.isPath x || builtins.isString x;
      merge = defs: builtins.elemAt defs 0;
    };

    listOf = elemType: {
      _type = "type";
      name = "listOf ${elemType.name}";
      check = x: builtins.isList x && builtins.all elemType.check x;
      merge = defs: builtins.concatLists defs;
    };

    attrsOf = elemType: {
      _type = "type";
      name = "attrsOf ${elemType.name}";
      check = x: builtins.isAttrs x && builtins.all elemType.check (builtins.attrValues x);
      merge = defs: builtins.foldl' (a: b: a // b) {} defs;
    };

    submodule = modules: {
      _type = "type";
      name = "submodule";
      inherit modules;
      check = x: builtins.isAttrs x;
      merge = defs: builtins.foldl' (a: b: a // b) {} defs;
    };

    enum = values: {
      _type = "type";
      name = "enum [${builtins.concatStringsSep " " (map (v: ''"${v}"'') values)}]";
      check = x: builtins.elem x values;
      merge = defs: builtins.elemAt defs 0;
    };

    nullOr = elemType: {
      _type = "type";
      name = "nullOr ${elemType.name}";
      check = x: x == null || elemType.check x;
      merge = defs:
        let nonNull = builtins.filter (x: x != null) defs;
        in if nonNull == [] then null else builtins.elemAt nonNull 0;
    };

    either = t1: t2: {
      _type = "type";
      name = "either ${t1.name} ${t2.name}";
      check = x: t1.check x || t2.check x;
      merge = defs: builtins.elemAt defs 0;
    };

    package = {
      _type = "type";
      name = "package";
      check = x: builtins.isAttrs x && x ? type && x.type == "derivation";
      merge = defs: builtins.elemAt defs 0;
    };

    port = {
      _type = "type";
      name = "port";
      check = x: builtins.isInt x && x >= 0 && x <= 65535;
      merge = defs: builtins.elemAt defs 0;
    };

    lines = {
      _type = "type";
      name = "lines";
      check = x: builtins.isString x;
      merge = defs: builtins.concatStringsSep "\n" defs;
    };
  };

  # Option declaration
  mkOption = {
    type ? types.str,
    default ? null,
    defaultText ? null,
    example ? null,
    description ? null,
    internal ? false,
    visible ? true,
    readOnly ? false,
    apply ? x: x,
  }: {
    _type = "option";
    inherit type default defaultText example description internal visible readOnly apply;
  };

  # Conditional definition
  mkIf = condition: content: {
    _type = "if";
    inherit condition content;
  };

  # Merge multiple definitions
  mkMerge = contents: {
    _type = "merge";
    inherit contents;
  };

  # Priority modifiers
  mkDefault = content: mkOverride 1000 content;
  mkForce = content: mkOverride 50 content;
  mkOverride = priority: content: {
    _type = "override";
    inherit priority content;
  };

  # Enable option shorthand
  mkEnableOption = description: mkOption {
    type = types.bool;
    default = false;
    inherit description;
  };

  # Package option shorthand
  mkPackageOption = pkgs: name: {
    default ? [name],
    example ? null,
  }: mkOption {
    type = types.package;
    default = pkgs.${builtins.head default} or null;
    defaultText = "pkgs.${builtins.head default}";
    description = "The ${name} package to use.";
    inherit example;
  };

  # Helper to create derived options
  mkDerivedConfig = options: f: mkOverride 1250 (f options);

  # Literal expression for documentation
  literalExpression = text: {
    _type = "literalExpression";
    inherit text;
  };

  literalExample = text: literalExpression text;

  # Module composition
  mkModuleOptions = attrs: attrs;

  # Filter attributes
  filterAttrs = pred: attrs:
    builtins.listToAttrs (
      builtins.filter
        (x: pred x.name x.value)
        (builtins.map (name: { inherit name; value = attrs.${name}; }) (builtins.attrNames attrs))
    );

  # Map attributes
  mapAttrs = f: attrs:
    builtins.listToAttrs (
      builtins.map
        (name: { inherit name; value = f name attrs.${name}; })
        (builtins.attrNames attrs)
    );

  # Recursive update
  recursiveUpdate = lhs: rhs:
    let
      isAttrs = x: builtins.isAttrs x && !(x ? _type);
    in
    lhs // builtins.mapAttrs (name: value:
      if isAttrs value && isAttrs (lhs.${name} or null)
      then recursiveUpdate lhs.${name} value
      else value
    ) rhs;

  # Optional attributes
  optionalAttrs = cond: attrs: if cond then attrs else {};
  optionalString = cond: str: if cond then str else "";
  optional = cond: elem: if cond then [elem] else [];

  # Assert with message
  assertMsg = cond: msg: if cond then true else builtins.throw msg;
}
