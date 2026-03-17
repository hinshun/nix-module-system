# lib.nix - Module system library functions
#
# This provides mkOption, mkIf, types, evalModules, and other helpers.
# Requires the nix-module-plugin to be loaded (provides builtins.nms_*).
#
# NOTE: All primop arguments and return values are JSON strings.
# The Nix C API EvalState pointer in primop callbacks is a raw nix::EvalState*,
# not the C API wrapper, so C API functions that need EvalState segfault.
# JSON serialization sidesteps this entirely — primops only read/write strings.

rec {
  # =========================================================================
  # Internal: serialize type descriptor to JSON for primops
  # =========================================================================
  #
  # Type descriptors contain functions (check, merge) that can't be serialized.
  # This strips them, keeping only the fields Rust needs for type resolution.

  _serializeType = type:
    { inherit (type) _type name; }
    // (if type ? elemType then { elemType = _serializeType type.elemType; } else {})
    // (if type ? values then { inherit (type) values; } else {})
    // (if type ? t1 then { t1 = _serializeType type.t1; } else {})
    // (if type ? t2 then { t2 = _serializeType type.t2; } else {});

  # =========================================================================
  # Type definitions
  # =========================================================================
  #
  # Each type is an attrset with:
  #   _type = "type";
  #   name  = "<type-name>";
  #   check = value -> bool;
  #   merge = name -> defs -> value;
  #
  # Type checking and merging delegate to Rust primops via JSON strings.
  # checkType: builtins.nms_checkType typeDescJSON valueJSON -> bool
  # mergeDefinitions: builtins.nms_mergeDefinitions name typeDescJSON defsJSON -> resultJSON

  types = {
    bool = {
      _type = "type";
      name = "bool";
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType types.bool)) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.bool)) (builtins.toJSON defs));
    };

    int = {
      _type = "type";
      name = "int";
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType types.int)) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.int)) (builtins.toJSON defs));
    };

    str = {
      _type = "type";
      name = "str";
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType types.str)) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.str)) (builtins.toJSON defs));
    };

    path = {
      _type = "type";
      name = "path";
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType types.path)) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.path)) (builtins.toJSON defs));
    };

    float = {
      _type = "type";
      name = "float";
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType types.float)) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.float)) (builtins.toJSON defs));
    };

    listOf = elemType: {
      _type = "type";
      name = "listOf ${elemType.name}";
      inherit elemType;
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType (types.listOf elemType))) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType (types.listOf elemType))) (builtins.toJSON defs));
    };

    attrsOf = elemType: {
      _type = "type";
      name = "attrsOf ${elemType.name}";
      inherit elemType;
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType (types.attrsOf elemType))) (builtins.toJSON x);
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType (types.attrsOf elemType))) (builtins.toJSON defs));
    };

    submodule = modules: {
      _type = "type";
      name = "submodule";
      inherit modules;
      check = x: builtins.isAttrs x;
      merge = name: defs:
        builtins.foldl' (a: b: a // b) {} (map (d: d.value) defs);
    };

    enum = values: {
      _type = "type";
      name = "enum [${builtins.concatStringsSep " " (map (v: ''"${v}"'') values)}]";
      inherit values;
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType (types.enum values))) (builtins.toJSON x);
      merge = name: defs:
        let vs = map (d: d.value) defs;
            first = builtins.elemAt vs 0;
        in if builtins.all (v: v == first) vs then first
           else builtins.throw "conflicting definitions for '${name}'";
    };

    nullOr = elemType: {
      _type = "type";
      name = "nullOr ${elemType.name}";
      inherit elemType;
      check = x: builtins.nms_checkType (builtins.toJSON (_serializeType (types.nullOr elemType))) (builtins.toJSON x);
      merge = name: defs:
        let nonNull = builtins.filter (d: d.value != null) defs;
        in if nonNull == [] then null
           else elemType.merge name nonNull;
    };

    either = t1: t2: {
      _type = "type";
      name = "either ${t1.name} ${t2.name}";
      inherit t1 t2;
      check = x: t1.check x || t2.check x;
      merge = name: defs:
        let first = (builtins.elemAt defs 0).value;
        in if t1.check first then t1.merge name defs
           else t2.merge name defs;
    };

    package = {
      _type = "type";
      name = "package";
      check = x: builtins.isAttrs x && x ? type && x.type == "derivation";
      merge = name: defs: (builtins.elemAt defs 0).value;
    };

    port = {
      _type = "type";
      name = "port";
      check = x: builtins.isInt x && x >= 0 && x <= 65535;
      merge = name: defs: builtins.fromJSON (builtins.nms_mergeDefinitions name (builtins.toJSON (_serializeType types.port)) (builtins.toJSON defs));
    };

    lines = {
      _type = "type";
      name = "lines";
      check = x: builtins.isString x;
      merge = name: defs:
        builtins.concatStringsSep "\n" (map (d: d.value) defs);
    };
  };

  # =========================================================================
  # Option declaration
  # =========================================================================

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

  # =========================================================================
  # Conditional and priority wrappers
  # =========================================================================

  mkIf = condition: content: {
    _type = "if";
    inherit condition content;
  };

  mkMerge = contents: {
    _type = "merge";
    inherit contents;
  };

  mkDefault = content: mkOverride 1000 content;
  mkForce = content: mkOverride 50 content;
  mkOverride = priority: content: {
    _type = "override";
    inherit priority content;
  };

  # =========================================================================
  # Convenience helpers
  # =========================================================================

  mkEnableOption = description: mkOption {
    type = types.bool;
    default = false;
    inherit description;
  };

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

  mkDerivedConfig = options: f: mkOverride 1250 (f options);

  literalExpression = text: {
    _type = "literalExpression";
    inherit text;
  };

  literalExample = text: literalExpression text;

  # =========================================================================
  # Process conditionals — delegates to Rust primop (JSON in, JSON out)
  # =========================================================================

  processConditionals = value:
    builtins.fromJSON (builtins.nms_processConditionals (builtins.toJSON value));

  # =========================================================================
  # evalModules — Nix-side orchestrator
  # =========================================================================
  #
  # This is the core function that evaluates a set of NixOS-style modules.
  # It handles:
  #   1. Module normalization (functions, imports, shorthand)
  #   2. Option collection from all modules
  #   3. Definition collection and conditional processing
  #   4. Merging via Rust primops
  #   5. Fixed-point evaluation via lazy recursion

  evalModules = {
    modules ? [],
    specialArgs ? {},
  }:
    let
      # Normalize a module: if it's a function, call it with module args.
      # If it's an attrset, use it directly.
      normalizeModule = args: m:
        if builtins.isFunction m then
          normalizeModule args (m args)
        else if builtins.isAttrs m then
          let
            hasOptions = m ? options;
            hasConfig = m ? config;
            hasImports = m ? imports;
          in
          if hasOptions || hasConfig || hasImports then m
          else { config = m; }  # shorthand: bare attrset = config
        else if builtins.isPath m then
          normalizeModule args (import m)
        else
          builtins.throw "module must be a function, attrset, or path; got ${builtins.typeOf m}";

      # Collect all modules (resolve imports recursively)
      collectModules = args: mods:
        builtins.concatLists (map (m:
          let normalized = normalizeModule args m;
              imports = normalized.imports or [];
          in [normalized] ++ collectModules args imports
        ) mods);

      # Extract all option declarations from collected modules
      collectOptions = modules:
        builtins.foldl' (acc: m:
          if m ? options then recursiveUpdate acc m.options else acc
        ) {} modules;

      # Extract all config definitions from collected modules, grouped by option path
      collectConfig = modules:
        builtins.foldl' (acc: m:
          let cfg = m.config or {};
          in recursiveUpdate acc cfg
        ) {} modules;

      # Merge a single option: process conditionals, then merge with type
      mergeOption = name: optDecl: rawDefs:
        let
          # Get all definitions for this option across modules
          defs =
            if builtins.isList rawDefs then
              builtins.concatLists (map (d:
                processConditionals d
              ) rawDefs)
            else
              processConditionals rawDefs;

          type = optDecl.type or types.str;
          apply = optDecl.apply or (x: x);
          default = optDecl.default or null;
          hasDefault = optDecl ? default;
        in
          if defs == [] || defs == null then
            if hasDefault then apply default
            else builtins.throw "option '${name}' is used but not defined and has no default value"
          else
            apply (type.merge name defs);

      # Fixed-point: config depends on options, modules depend on config
      result = let
        moduleArgs = {
          inherit (result) config options;
          lib = import ./lib.nix;
        } // specialArgs;

        allModules = collectModules moduleArgs modules;
        options = collectOptions allModules;
        rawConfig = collectConfig allModules;
      in {
        inherit options;

        # Merge each option with its definitions
        config = mapAttrs (name: optDecl:
          let rawDef = rawConfig.${name} or null;
          in if rawDef != null then
            mergeOption name optDecl rawDef
          else if optDecl ? default then
            (optDecl.apply or (x: x)) optDecl.default
          else null
        ) options;
      };

    in result;

  # =========================================================================
  # Utility functions
  # =========================================================================

  mkModuleOptions = attrs: attrs;

  filterAttrs = pred: attrs:
    builtins.listToAttrs (
      builtins.filter
        (x: pred x.name x.value)
        (builtins.map (name: { inherit name; value = attrs.${name}; }) (builtins.attrNames attrs))
    );

  mapAttrs = f: attrs:
    builtins.listToAttrs (
      builtins.map
        (name: { inherit name; value = f name attrs.${name}; })
        (builtins.attrNames attrs)
    );

  map = builtins.map;

  recursiveUpdate = lhs: rhs:
    let
      isAttrs = x: builtins.isAttrs x && !(x ? _type);
    in
    lhs // builtins.mapAttrs (name: value:
      if isAttrs value && isAttrs (lhs.${name} or null)
      then recursiveUpdate lhs.${name} value
      else value
    ) rhs;

  optionalAttrs = cond: attrs: if cond then attrs else {};
  optionalString = cond: str: if cond then str else "";
  optional = cond: elem: if cond then [elem] else [];

  assertMsg = cond: msg: if cond then true else builtins.throw msg;
}
