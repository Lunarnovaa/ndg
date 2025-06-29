{
  lib,
  rustPlatform,
  installShellFiles,
  stdenv,
}: let
  fs = lib.fileset;
  s = ../../..;
in
  rustPlatform.buildRustPackage (finalAttrs: {
    pname = "ndg";
    version = (builtins.fromTOML (builtins.readFile (s + /Cargo.toml))).package.version;

    nativeBuildInputs = [installShellFiles];

    src = fs.toSource {
      root = s;
      fileset = fs.unions [
        (fs.fileFilter (file: builtins.any file.hasExt ["rs"]) (s + /src))
        (s + /Cargo.lock)
        (s + /Cargo.toml)
        (s + /templates)
      ];
    };

    cargoLock.lockFile = "${finalAttrs.src}/Cargo.lock";
    useFetchCargoVendor = true;
    enableParallelBuilding = true;

    postInstall =
      lib.optionalString
      (stdenv.hostPlatform.canExecute stdenv.targetPlatform) ''
        $out/bin/ndg generate
        installShellCompletion dist/completions/{ndg.bash,ndg.fish,_ndg}
        installManPage dist/man/ndg.1
      '';

    meta = {
      description = "not a docs generator";
      homepage = "https://github.com/feel-co/ndg";
      license = lib.licenses.mpl20;
      mainProgram = "ndg";
    };
  })
