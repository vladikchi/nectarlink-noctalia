# NectarLink для Noctalia

[NectarLink](https://github.com/thtbee/nectarlink) связывает Android-телефон с компьютером: передаёт файлы, синхронизирует буфер обмена и выводит экран телефона на ПК. Эта сборка расширяет локальный API, через который [плагин NectarLink](https://github.com/vladikchi/nectarlink-plugins) может читать уведомления, отправлять SMS, управлять звонками и запускать трансляцию экрана или веб-камеру из панели Noctalia.

**Для плагина установите эту сборку:** в оригинальном NectarLink нет нужных ему маршрутов API. Кроме того, в этом форке тема Hive может использовать палитру Noctalia.

## Установка на Linux

В [последнем релизе](https://github.com/vladikchi/nectarlink-noctalia/releases/latest) доступны `.deb`, `.rpm`, архив и AppImage для Linux x86‑64.

### Установщик (`.deb`, `.rpm`, архив)

Установка одной командой:

```sh
curl -fsSL https://github.com/vladikchi/nectarlink-noctalia/releases/latest/download/install.sh | sh
```

Установщик выбирает `.deb` для APT, `.rpm` для DNF или Zypper либо архив и проверяет SHA-256 пакета перед установкой. Пакеты скачиваются из этого форка, не из оригинального NectarLink.

### AppImage

[Скачайте AppImage для Linux x86‑64](https://github.com/vladikchi/nectarlink-noctalia/releases/download/v0.1.5%2Bnoctalia.1/Nectarlink-0.1.5-noctalia.1-x86_64.AppImage), разрешите выполнение и запустите. Системная установка не нужна:

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

Сопряжение, включение API и установка виджета описаны в [руководстве по установке плагина](https://github.com/vladikchi/nectarlink-plugins#установка).

## Что изменено относительно оригинала

- **Локальный API:** добавлены чтение уведомлений и разделов панели, команды для телефона и управление сессиями. Для камеры и отправки данных неверная явно указанная цель больше не подменяется другим телефоном.
- **Палитра Noctalia:** тема Hive может читать готовый `~/.config/nectarlink/noctalia-colors.json`. Приложение не создаёт этот файл. Без подходящей палитры Hive использует встроенные цвета.
- **Обновление и упаковка:** обновление из оригинального проекта отключено на Linux. Перед ручным обновлением закройте NectarLink, затем замените AppImage или установите новый пакет форка. Профиль хранится отдельно. При упаковке AppImage проверяется наличие русского перевода.

Исправления Island, ввода с телефона и захвата экрана вынесены в [NectarLink для Niri](https://github.com/vladikchi/nectarlink-niri) и не входят в сборку для Noctalia.

[Все изменения, маршруты API и файлы реализации →](CHANGES.md)

## Проверки и лицензии

Локальный AppImage на Arch прошёл `--version` и `--ui-check`; русский перевод загрузился. Это не проверка с реальным телефоном, порталом Wayland или другим дистрибутивом.

Основа форка — NectarLink `v0.1.6-nightly.10`. [Оригинальный проект](https://github.com/thtbee/nectarlink) · [Лицензии компонентов](LICENSE)
