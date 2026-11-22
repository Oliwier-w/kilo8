# 01. Architektura i pamięć

## Granica między emulatorem a systemem

```text
plik / klawiatura ──> aplikacja ──> scheduler ──> maszyna CHIP-8
                         ^                         |
                         +── stan do odczytu  <─────+
                         |                         |
                         v                         v
                 ekran i debugger              sygnał ST > 0
                                                   |
                                                   v
                                             generator audio
```

Maszyna przechowuje procesor, RAM, ekran, klawisze, timery i profil zgodności. Jedno wywołanie kroku pobiera instrukcję pod PC, dekoduje ją i aktualizuje stan. Oczekiwanie na puszczenie klawisza w Fx0A zwraca stan oczekiwania do schedulera. Warstwa desktopowa czyta pliki i mapuje klawisze hosta. Debugger korzysta z tego samego dekodera co procesor i odczytuje stan bez wykonywania instrukcji. Generator dźwięku dostaje sygnał bramkujący i może generować falę około 440 Hz bez plików audio.

## Plan katalogów

Gwiazdka oznacza plik planowany na dalsze etapy.

```text
kilo8/
|-- Cargo.toml
|-- Cargo.lock
|-- LICENSE
|-- README.md
|-- roms/
|   `-- README.md
|-- docs/
|   |-- 01_architecture_and_memory.md
|   |-- 02_cpu_opcodes_bitwise.md *
|   |-- 03_xor_graphics.md *
|   |-- 04_timers.md *
|   |-- 05_keypad.md *
|   |-- 06_quirks.md *
|   |-- 07_scheduler.md *
|   |-- 08_audio.md *
|   `-- 09_desktop_and_debugger.md *
`-- src/
    |-- lib.rs
    |-- main.rs *
    |-- audio.rs *
    |-- scheduler.rs *
    |-- core/
    |   |-- mod.rs
    |   |-- cpu.rs *
    |   |-- memory.rs
    |   |-- display.rs *
    |   |-- timers.rs *
    |   |-- keypad.rs *
    |   `-- quirks.rs
    `-- ui/
        |-- mod.rs *
        |-- display.rs *
        `-- debugger.rs *
```

Krok 1 kompiluje bibliotekę bez zależności okienkowych. Opcja `desktop` włącza eframe z rendererem OpenGL oraz rfd do dialogów. Opcja `audio` dodatkowo włącza odtwarzanie przez rodio bez dekoderów formatów audio. Planowany moduł CPU otrzyma źródło losowego bajtu dla `Cxkk` przez parametr, co pozwoli testować je z ustaloną wartością.

## Trzy częstotliwości

Aplikacja odmierza czas monotonicznym `Instant`. Za każdym przebudzeniem od poprzedniego pomiaru liczy upływ w nanosekundach. Scheduler przechowuje osobne fazy CPU i timerów jako liczby całkowite. Dla stałej częstotliwości zdarzenia przypadają na następujące progi fazy.

```text
faza += delta_ns * częstotliwość_hz
liczba_zdarzeń = faza / 1_000_000_000
faza %= 1_000_000_000
```

Reszta przechodzi na następne przebudzenie. Dzięki temu przy 700 Hz na klatkę zwykle przypada 11 lub 12 instrukcji, a przy 60 Hz oba timery mają 60 okazji do dekrementacji na sekundę. Timery zatrzymują się na zerze. Użyj `u128` do mnożenia i ustal kolejność zdarzeń, porównując chwile przekroczenia progów CPU i timerów wewnątrz okna czasu. Gdy progi wypadają jednocześnie, wykonaj tick timerów przed instrukcją. Zachowuje to kolejność zapisów do DT i ST względem dekrementacji.

Renderowanie celuje w 60 FPS za pomocą `request_repaint_after` i deadline'u przesuwanego o okres od pierwotnej epoki. Dokładny deadline klatki n to epoka plus `floor(n * 1_000_000_000 / 60)` nanosekund. Wybudzenia mogą być opóźnione przez system. Scheduler nadgania krótkie opóźnienia, a po przekroczeniu limitu, np. 100 ms, celowo odrzuca nadmiar czasu i wykonuje najwyżej 70 instrukcji przy 700 Hz. To ogranicza koszt jednej aktualizacji. Przy pauzie debuggera oba zegary zamierają, pojedynczy krok wykonuje jedną próbę instrukcji, a wznowienie odświeża punkt pomiaru. Podczas blokady Fx0A niewykorzystane sloty CPU przepadają, ale timery nadal pracują. Na zmianę Hz rozlicz czas do chwili zmiany po starej częstotliwości i zachowaj ułamek cyklu CPU.

## Mapa adresów

```text
0x000..0x04F  obszar przed fontem
0x050..0x09F  font 0..F, 16 glifów po 5 bajtów
0x0A0..0x1FF  pozostały obszar poniżej programu
0x200..0xFFF  program i dane, do 3584 bajtów przy ładowaniu
```

Adres RAM ma 12 bitów. Kod hosta przechowuje adres jako `u16`, więc pamięć zgłasza błąd także dla wartości większych niż 0xFFF. Zakres o długości zero może zaczynać się pod 0x1000. Wbudowany font trafia do zwykłej zapisywalnej pamięci podczas inicjalizacji. Planowana instrukcja Fx29 ustawia I na `FONT_START + u16::from(vx & 0x0F) * GLYPH_SIZE`. Wiersz `0x90` ma postać `1001_0000` i oznacza zapalone skrajne piksele czteropikselowego glifu.

Instrukcja zajmuje dwa bajty w kolejności big-endian. Odczyt spod 0xFFE jest poprawny, a pod 0xFFF brakuje drugiego bajtu. Instrukcję można wydobyć także spod nieparzystego adresu. Na przykład `0xD0, 0x14` daje `DRW V0, V1, 4`.

```rust
let word = (u16::from(0xD0_u8) << 8) | u16::from(0x14_u8);
let kind = (word >> 12) & 0xF;
let x = (word >> 8) & 0xF;
let y = (word >> 4) & 0xF;
let height = word & 0xF;
assert_eq!((kind, x, y, height), (0xD, 0, 1, 4));

let row = 0x90_u8;
assert_ne!(row & (0x80 >> 3), 0);
assert!(!(true ^ true)); // XOR gasi zapalony piksel i daje kolizję
```

CPU powinien sprawdzić pełny zakres przed `Fx55`, `Fx65` i rysowaniem sprite'a. Dopiero po sukcesie transferu zmienia I według ustawienia zgodności. Walidacja zakresu w jednym miejscu zapobiega częściowym zapisom i przepełnieniu przy obliczaniu końca. `load_rom` zastępuje RAM dopiero po sprawdzeniu rozmiaru. Planowany reset maszyny odtwarza RAM z zachowanego obrazu ROM i inicjalizuje również rejestry, ekran, stos, klawisze i timery.

Debugger przycina zakres podglądu do RAM, pokazując adresy w zapisie szesnastkowym. Błąd pobrania opcode'u może pokazać przy PC bez zatrzymywania samego debuggera.
