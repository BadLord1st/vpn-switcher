# vpn-switcher

HTTP-микросервис для цикличного переключения VPN-профилей (пока только Outline через `outline-cli` wrapper-команду `vpn`).

## Что делает

- `POST /switch` переключает на следующий профиль из списка, который сервис читает из `outline-cli`
- сохраняет состояние в `STATE_PATH` (последний клиент и индекс профиля)
- после рестарта продолжает цикл с последнего состояния

## Переменные окружения

- `LISTEN_ADDR` (опционально): адрес сервера, по умолчанию `0.0.0.0:8080`
- `STATE_PATH` (опционально): путь к JSON состоянию, по умолчанию `./state/vpn-switcher-state.json`
- `OUTLINE_COMMAND_BIN` (опционально): исполняемая команда, по умолчанию `vpn`
- `OUTLINE_LIST_ARGS` (опционально): аргументы для чтения профилей, по умолчанию `list -f %name%`
- `OUTLINE_COMMAND_PREFIX` (опционально): аргументы до имени профиля, по умолчанию `connect`
- `TOKIO_WORKER_THREADS` (опционально): число worker threads, автоматически ограничивается в диапазон `1..2`

## Запуск

```bash
export OUTLINE_COMMAND_BIN="vpn"
export OUTLINE_LIST_ARGS="list -f %name%"
export OUTLINE_COMMAND_PREFIX="connect"
cargo run
```

Если нужно вызывать через `pkexec`, можно так:

```bash
export OUTLINE_COMMAND_BIN="pkexec"
export OUTLINE_COMMAND_PREFIX="/usr/local/bin/__vpn_manager connect -n"
cargo run
```

## Установка как systemd-демон

В репозитории есть:

- unit: `/deploy/systemd/vpn-switcher.service`
- env-шаблон: `/config/vpn-switcher.env.example`
- установщик: `/scripts/install-systemd.sh`

Важно: сервис запускается от `root` (это нужно для VPN-команд, требующих привилегий).
Это уже зафиксировано в unit-файле: `User=root`, `Group=root`.

Установка:

```bash
cd vpn-switcher
sudo ./scripts/install-systemd.sh
```

Что делает установщик:

- собирает `cargo build --release`
- копирует бинарник в `/usr/local/bin/vpn-switcher`
- ставит unit в `/etc/systemd/system/vpn-switcher.service`
- создаёт `/etc/vpn-switcher/vpn-switcher.env` (только при первом запуске)
- включает и запускает сервис (`systemctl enable --now`)

После установки проверь и при необходимости поправь конфиг:

```bash
sudoedit /etc/vpn-switcher/vpn-switcher.env
sudo systemctl restart vpn-switcher
sudo systemctl status vpn-switcher
journalctl -u vpn-switcher -f
```

## API

### health

```bash
curl -s http://127.0.0.1:8080/healthz
```

### текущее состояние

```bash
curl -s http://127.0.0.1:8080/state | jq
```

### переключить на следующий профиль

```bash
curl -s -X POST http://127.0.0.1:8080/switch | jq
```

## Примечание

Сервис выполняет команды последовательно (под mutex), чтобы избежать гонок при одновременных запросах на переключение.

## License

MIT
