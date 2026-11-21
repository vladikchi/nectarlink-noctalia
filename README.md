# NectarLink для Noctalia

Форк NectarLink с расширенным локальным API для [плагина Noctalia](https://github.com/vladikchi/nectarlink-plugins) и поддержкой внешней палитры Noctalia в теме Hive. Плагин использует этот API для уведомлений, сообщений, звонков, фотографий и управления телефоном из панели Noctalia. В оригинальном NectarLink нужных плагину маршрутов нет.

## Установка на Linux

Готовые пакеты для Linux x86‑64 ищите в [релизах форка](https://github.com/vladikchi/nectarlink-noctalia/releases). Если релиз ещё недоступен, можно собрать приложение из исходников.

### Установщик (`.deb`, `.rpm`, архив)

Если в последнем релизе есть `install.sh`, установите приложение одной командой:

```sh
curl -fsSL https://github.com/vladikchi/nectarlink-noctalia/releases/latest/download/install.sh | sh
```

Установщик выбирает `.deb` для APT, `.rpm` для DNF или Zypper либо архив и проверяет SHA-256 пакета перед установкой. Пакеты скачиваются из этого форка, не из оригинального NectarLink.

### AppImage

AppImage запускается без системной установки. Скачайте `.AppImage` из релиза и запустите его. Например, для `0.1.5+noctalia.1`:

```sh
chmod +x ./Nectarlink-0.1.5-noctalia.1-x86_64.AppImage
./Nectarlink-0.1.5-noctalia.1-x86_64.AppImage
```

### Сборка из исходников

Нужны Rust, инструменты разработки Qt 6, C++-компилятор, CMake, Ninja и системные библиотеки. Из корня репозитория:

```sh
cargo build --release --locked --package nectarlink-desktop
./target/release/nectarlink-desktop
```

**Android:** [скачать официальный APK NectarLink](https://github.com/thtbee/nectarlink/releases/download/v0.1.5/Nectarlink-0.1.5-android.apk). На телефоне используется оригинальный клиент, форк нужен только на ПК.

Подключение описано в [руководстве плагина](https://github.com/vladikchi/nectarlink-plugins/blob/main/nectarlink/README.md).

## Что изменено относительно оригинала

- **Локальный API:** добавлены чтение уведомлений и разделов панели, команды для телефона и управление сессиями. Для камеры и отправки данных неверная явно указанная цель больше не подменяется другим телефоном.
- **Палитра Noctalia:** тема Hive может читать готовый `~/.config/nectarlink/noctalia-colors.json`. Приложение не создаёт этот файл. Без подходящей палитры Hive использует встроенные цвета.
- **Обновление и упаковка:** обновление из оригинального проекта отключено на Linux. Перед ручным обновлением закройте NectarLink, затем замените AppImage или установите новый пакет форка. Профиль хранится отдельно. При упаковке AppImage проверяется наличие русского перевода.

Исправления Island, ввода с телефона и захвата экрана вынесены в [NectarLink для Niri](https://github.com/vladikchi/nectarlink-niri) и не входят в сборку для Noctalia.

[Все изменения, маршруты API и файлы реализации →](CHANGES.md)

## Проверки и лицензии

Локальный AppImage на Arch прошёл `--version` и `--ui-check`; русский перевод загрузился. Это не проверка с реальным телефоном, порталом Wayland или другим дистрибутивом.

Основа форка — NectarLink `v0.1.6-nightly.10`. [Оригинальный проект](https://github.com/thtbee/nectarlink) · [Лицензии компонентов](LICENSE)
