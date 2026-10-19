# UNHUB

Versão em Rust do LGLITE: configura mouses Logitech G com fio sem o G HUB. Testado no **G403 HERO (046D:C08F)**.
A interface segue o layout do Claude Design (`../layout/UNHUB.dc.html`).

| Função | Onde roda | Precisa do UNHUB aberto? |
|---|---|---|
| Atalhos: botões do mouse, DPI, tecla + modificadores, mídia, G-Shift, desativar | memória do mouse | não |
| DPI: até 5 níveis, padrão, shift, taxa de relatório, "Testar agora" | memória do mouse | não |
| Iluminação: desligado, fixo, respiração, ciclo de cores, por zona | memória do mouse | não |
| Macros: 4 tipos do G HUB, teclas, texto, cliques, rolagem, atrasos, mídia, programas; gravação; importação do G HUB | agente na bandeja | só o agente |

## Uso

```
unhub.exe              interface
unhub.exe --agent      agente de macros (o botão "Iniciar" da barra lateral faz o mesmo)
unhub.exe --status     mostra o estado do mouse no terminal (só leitura)
```

Para compilar: `cargo build --release` (gera `target\release\unhub.exe`). Testes: `cargo test`.

**Feche o G HUB** (inclusive o ícone da bandeja) antes de aplicar: ele põe o mouse em modo host e ignora o perfil onboard. O UNHUB avisa quando o G HUB está aberto.

## Dados

`%APPDATA%\UNHUB\config.json` (macros e gatilhos) e `%APPDATA%\UNHUB\backups\`. Na primeira execução o UNHUB copia
`%APPDATA%\LGLITE` para lá, então as macros e o backup do perfil original da versão Python continuam valendo.
O formato do `config.json` é o mesmo do LGLITE.

## Agente de macros

Igual ao LGLITE: o botão com macro recebe, na memória do mouse, uma tecla sentinela de F13 a F24. O agente
(`unhub.exe --agent`, ícone na bandeja) captura essa tecla com um hook de teclado, engole e toca a macro com
`SendInput` (teclado por scan code). Tudo o que a macro deixou pressionado é solto ao terminar. Limite de 12 botões
com macro. Assim o mouse continua em modo onboard e DPI, cores e atalhos funcionam sem nenhum software.

- O agente relê `config.json` sozinho quando a interface salva.
- "Iniciar com o Windows" grava `HKCU\...\Run\UNHUB Agent` e remove a entrada `LGLITE Agent`, se existir.
- Não deixe o agente do LGLITE (Python) aberto junto: os dois capturam F13–F24. A interface avisa.
- Janelas de administrador só recebem as macros se o agente também rodar como administrador.

## Estrutura

| Arquivo | Conteúdo |
|---|---|
| `hidpp.rs` | transporte HID++ 2.0 (relatório 0x11, índice 0xFF), feature 0x2201 (DPI) |
| `onboard.rs` | feature 0x8100, formato do setor de perfil, botões, LEDs, CRC |
| `mouse.rs`, `device.rs` | sessão com o mouse, backup, thread que fala com o HID |
| `macros.rs`, `config.rs` | modelo de macros, importação do G HUB, sentinelas |
| `player.rs`, `winput.rs`, `agent.rs` | executor, SendInput/hook, agente da bandeja |
| `ui/` | interface (egui) |
