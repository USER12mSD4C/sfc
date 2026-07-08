#!/usr/bin/env bash

# Цвета для красивого вывода
RED='\e[31m'
GREEN='\e[32m'
YELLOW='\e[33m'
BLUE='\e[34m'
NC='\e[0m' # No Color

echo -e "${BLUE}=== Запуск побайтового тест-драйва SFC ===${NC}"

# Шаг 1. Лоцируем оригинальные GNU бинарники через Nix
echo -e "${YELLOW}[*] Поиск оригинальных GNU Coreutils в Nix Store...${NC}"
GNU_BIN=$(nix-build --no-out-link "<nixpkgs>" -A coreutils)/bin
if [ -z "$GNU_BIN" ]; then
    echo -e "${RED}[ERR] Не удалось найти GNU Coreutils через Nix.${NC}"
    exit 1
fi
echo -e "${GREEN}[OK] Найдено: ${GNU_BIN}${NC}"

# Папка с нашими свежескомпилированными Rust бинарниками
RUST_BIN="./target/release"
if [ ! -d "$RUST_BIN" ]; then
    echo -e "${RED}[ERR] Директория $RUST_BIN не найдена. Сначала соберите проект: cargo build --release${NC}"
    exit 1
fi

# Временная папка-песочница для тестов
SANDBOX=$(mktemp -d)
trap 'rm -rf "$SANDBOX"' EXIT

# Вспомогательная функция побайтового сравнения
compare() {
    local test_name=$1
    local gnu_cmd=$2
    local rust_cmd=$3

    # Выполняем GNU версию
    eval "$gnu_cmd" > "$SANDBOX/gnu.out" 2> "$SANDBOX/gnu.err"
    local gnu_code=$?

    # Выполняем нашу Rust версию
    eval "$rust_cmd" > "$SANDBOX/rust.out" 2> "$SANDBOX/rust.err"
    local rust_code=$?

    # 1. Побайтовое сравнение stdout
    if ! cmp -s "$SANDBOX/gnu.out" "$SANDBOX/rust.out"; then
        echo -e "  ${RED}[FAIL]${NC} $test_name (различие в байтах stdout)"
        echo -e "${YELLOW}--- Побайтовый дамп GNU (${gnu_cmd}) ---${NC}"
        hexdump -C "$SANDBOX/gnu.out"
        echo -e "${YELLOW}--- Побайтовый дамп Rust (${rust_cmd}) ---${NC}"
        hexdump -C "$SANDBOX/rust.out"
        return 1
    fi

    # 2. Сравнение кодов возврата (POSIX exit status)
    if [ "$gnu_code" -ne "$rust_code" ]; then
        echo -e "  ${RED}[FAIL]${NC} $test_name (код возврата отличается! GNU=$gnu_code, Rust=$rust_code)"
        return 1
    fi

    echo -e "  ${GREEN}[ OK ]${NC} $test_name"
    return 0
}

# Генерируем тестовый файл с бинарным и текстовым мусором
echo -e "Hello, SFC!\nThis is a multiline test\nwith some binary characters \x00\x01\x02\xff\n" > "$SANDBOX/test_input.bin"

# ==========================================
# Начинаем тесты
# ==========================================

# 1. Тесты SHA256SUM
echo -e "\n${BLUE}--- Тестирование sha256sum ---${NC}"
compare "sha256sum (файл)" "$GNU_BIN/sha256sum $SANDBOX/test_input.bin" "$RUST_BIN/sha256sum $SANDBOX/test_input.bin"
compare "sha256sum (stdin)" "cat $SANDBOX/test_input.bin | $GNU_BIN/sha256sum" "cat $SANDBOX/test_input.bin | $RUST_BIN/sha256sum"

# 2. Тесты ARCH
echo -e "\n${BLUE}--- Тестирование arch ---${NC}"
compare "arch" "$GNU_BIN/arch" "$RUST_BIN/arch"

# 3. Тесты PRINTF
echo -e "\n${BLUE}--- Тестирование printf ---${NC}"
compare "printf (базовый текст)" "$GNU_BIN/printf 'Hello %s\n' 'World'" "$RUST_BIN/printf 'Hello %s\n' 'World'"
compare "printf (escapes)" "$GNU_BIN/printf 'Row1\nRow2\tTabbed\\\n'" "$RUST_BIN/printf 'Row1\nRow2\tTabbed\\\n'"
compare "printf (hex/floats)" "$GNU_BIN/printf '%x %X %f\n' 255 1024 3.1415" "$RUST_BIN/printf '%x %X %f\n' 255 1024 3.1415"

# 4. Тесты DIRCOLORS
echo -e "\n${BLUE}--- Тестирование dircolors ---${NC}"
compare "dircolors (sh/bash)" "$GNU_BIN/dircolors" "$RUST_BIN/dircolors"
compare "dircolors (csh)" "$GNU_BIN/dircolors -c" "$RUST_BIN/dircolors -c"

# 5. Тесты EXPR
echo -e "\n${BLUE}--- Тестирование expr ---${NC}"
compare "expr (сложение)" "$GNU_BIN/expr 50 + 50" "$RUST_BIN/expr 50 + 50"
compare "expr (вычитание)" "$GNU_BIN/expr 200 - 75" "$RUST_BIN/expr 200 - 75"
compare "expr (умножение)" "$GNU_BIN/expr 12 \* 12" "$RUST_BIN/expr 12 \* 12"
compare "expr (деление)" "$GNU_BIN/expr 100 / 3" "$RUST_BIN/expr 100 / 3"
compare "expr (остаток)" "$GNU_BIN/expr 100 % 3" "$RUST_BIN/expr 100 % 3"
compare "expr (длина строки)" "$GNU_BIN/expr length 'osdev_rust'" "$RUST_BIN/expr length 'osdev_rust'"
compare "expr (сравнение строк)" "$GNU_BIN/expr 'nixos' = 'nixos'" "$RUST_BIN/expr 'nixos' = 'nixos'"

# 6. Тесты CKSUM
echo -e "\n${BLUE}--- Тестирование cksum ---${NC}"
compare "cksum (файл)" "$GNU_BIN/cksum $SANDBOX/test_input.bin" "$RUST_BIN/cksum $SANDBOX/test_input.bin"
compare "cksum (stdin)" "cat $SANDBOX/test_input.bin | $GNU_BIN/cksum" "cat $SANDBOX/test_input.bin | $RUST_BIN/cksum"

# 7. Тесты SUM
echo -e "\n${BLUE}--- Тестирование sum ---${NC}"
compare "sum (файл)" "$GNU_BIN/sum $SANDBOX/test_input.bin" "$RUST_BIN/sum $SANDBOX/test_input.bin"
compare "sum (stdin)" "cat $SANDBOX/test_input.bin | $GNU_BIN/sum" "cat $SANDBOX/test_input.bin | $RUST_BIN/sum"

# 8. Тесты RMDIR (требуют изоляции состояний папок)
echo -e "\n${BLUE}--- Тестирование rmdir ---${NC}"
# Успешное удаление пустой папки
mkdir "$SANDBOX/dir_gnu" "$SANDBOX/dir_rust"
compare "rmdir (удаление пустой папки)" "$GNU_BIN/rmdir $SANDBOX/dir_gnu" "$RUST_BIN/rmdir $SANDBOX/dir_rust"

# Симуляция ошибки (удаление несуществующей папки)
compare "rmdir (ошибка отсутствия папки)" "$GNU_BIN/rmdir $SANDBOX/non_existent_folder" "$RUST_BIN/rmdir $SANDBOX/non_existent_folder"

echo -e "\n${GREEN}=== Тест-драйв завершен! ===${NC}"
