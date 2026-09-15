# CapVault

Aplicativo desktop local para localizar, exportar e restaurar projetos editáveis do CapCut com segurança.

## Recursos

- Detecta automaticamente a pasta padrão do CapCut no Windows.
- Permite selecionar qualquer outra pasta raiz.
- Lista projetos com nome, tamanho, quantidade de arquivos e data de modificação.
- Pesquisa projetos pelo nome.
- Exporta a estrutura completa do projeto para `.capcutpkg` (ZIP).
- Valida o manifesto e os caminhos de todos os arquivos antes da importação.
- Extrai o pacote em uma pasta temporária e só conclui a importação após a validação.
- Nunca apaga ou substitui automaticamente um projeto existente.
- Preserva o nome original da pasta do projeto e cria um nome alternativo em caso de conflito.

## Limitação atual

O pacote contém os arquivos presentes na pasta interna do projeto. Vídeos, imagens, áudios e fontes referenciados em outros locais ainda não são copiados. Antes de remover um projeto ou migrá-lo para outro computador, confirme se todas as mídias necessárias estão disponíveis.

O CapVault não é afiliado, mantido ou endossado pela ByteDance ou pelo CapCut.

## Desenvolvimento

Requisitos:

- Node.js 20 ou mais recente
- Rust estável
- Dependências de desenvolvimento do [Tauri 2](https://v2.tauri.app/start/prerequisites/)

```bash
npm install
npm run tauri dev
```

## Verificação

```bash
npm run build
cd src-tauri
cargo test
cargo clippy --all-targets -- -D warnings
```

## Formato do pacote

Um `.capcutpkg` é um arquivo ZIP com a seguinte estrutura:

```text
capcut-package.json
project/
  draft_content.json
  draft_meta_info.json
  ...
```

O manifesto identifica a versão do formato, o nome visível do projeto, o nome original da pasta e a data de exportação.

## Segurança

- Links simbólicos não são incluídos na exportação nem aceitos na importação.
- Caminhos absolutos e tentativas de ZIP traversal são rejeitados.
- Pacotes sem os arquivos essenciais do CapCut são rejeitados.
- Importações são limitadas a 100.000 entradas e 500 GB descompactados.
- Um pacote incompleto não deixa arquivos parciais na biblioteca.

## Licença

[MIT](LICENSE)
