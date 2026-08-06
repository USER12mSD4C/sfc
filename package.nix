{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "sfc";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  doCheck = false;

  # Создаем симлинки после установки бинарей
  postInstall = ''
    ln -sf ls $out/bin/dir
    ln -sf ls $out/bin/vdir
    ln -sf touch $out/bin/mk
    ln -sf id $out/bin/whoami
  '';

  meta = {
    description = "SFC - Simple & Fast Coreutils in Rust";
    homepage = "https://github.com/user12msd4c/sfc";
    license = lib.licenses.mit;
    mainProgram = "sfsh"; # Исправлено с sfshell на sfsh
  };
}
