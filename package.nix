{ lib, rustPlatform }:

rustPlatform.buildRustPackage {
  pname = "sfc";
  version = "0.1.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  # ОТКЛЮЧАЕМ ФАЗУ ТЕСТИРОВАНИЯ (сэкономит половину времени сборки пакета)
  doCheck = false;

  postInstall = ''
    ln -s test $out/bin/[
  '';

  meta = {
    description = "SFC - Simple & Fast Coreutils in Rust";
    homepage = "https://github.com/user12msd4c/sfc";
    license = lib.licenses.mit;
    mainProgram = "sfshell";
  };
}
