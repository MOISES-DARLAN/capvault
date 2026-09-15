import { useDeferredValue, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import "./App.css";

type Project = {
  name: string;
  path: string;
  sizeBytes: number;
  fileCount: number;
  modifiedAt: number;
};

type ImportResult = {
  projectName: string;
  destination: string;
};

const ROOT_STORAGE_KEY = "capvault.root";

function formatBytes(bytes: number) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value.toLocaleString("pt-BR", { maximumFractionDigits: index ? 1 : 0 })} ${units[index]}`;
}

function safeFileName(name: string) {
  return name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim() || "Projeto CapCut";
}

function FolderIcon() {
  return <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6.5A2.5 2.5 0 0 1 5.5 4H9l2 2h7.5A2.5 2.5 0 0 1 21 8.5v8a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5v-10Z" /></svg>;
}

function App() {
  const [root, setRoot] = useState(() => localStorage.getItem(ROOT_STORAGE_KEY) ?? "");
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedPath, setSelectedPath] = useState("");
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [busy, setBusy] = useState("");
  const [status, setStatus] = useState("Selecione a pasta de projetos para começar.");
  const [statusType, setStatusType] = useState<"info" | "success" | "error">("info");

  async function refresh(targetRoot = root) {
    if (!targetRoot) return;
    setBusy("Atualizando projetos");
    try {
      const found = await invoke<Project[]>("list_projects", { root: targetRoot });
      setProjects(found);
      setSelectedPath((current) => found.some((project) => project.path === current) ? current : "");
      setStatus(found.length ? `${found.length} projeto${found.length === 1 ? "" : "s"} encontrado${found.length === 1 ? "" : "s"}.` : "Nenhum projeto compatível encontrado nesta pasta.");
      setStatusType("info");
    } catch (error) {
      setProjects([]);
      setStatus(String(error));
      setStatusType("error");
    } finally {
      setBusy("");
    }
  }

  useEffect(() => {
    let active = true;
    async function initialize() {
      let initialRoot = localStorage.getItem(ROOT_STORAGE_KEY) ?? "";
      if (!initialRoot) {
        initialRoot = (await invoke<string | null>("detect_default_root")) ?? "";
        if (initialRoot && active) {
          setRoot(initialRoot);
          localStorage.setItem(ROOT_STORAGE_KEY, initialRoot);
        }
      }
      if (initialRoot && active) await refresh(initialRoot);
    }
    initialize();
    return () => { active = false; };
  }, []);

  async function chooseRoot() {
    const selected = await open({ directory: true, multiple: false, title: "Selecione a pasta de projetos do CapCut" });
    if (!selected) return;
    setRoot(selected);
    localStorage.setItem(ROOT_STORAGE_KEY, selected);
    await refresh(selected);
  }

  async function exportSelected() {
    const project = projects.find((item) => item.path === selectedPath);
    if (!project) return;
    const destination = await save({
      title: "Exportar projeto",
      defaultPath: `${safeFileName(project.name)}.capcutpkg`,
      filters: [{ name: "Pacote CapVault", extensions: ["capcutpkg", "zip"] }],
    });
    if (!destination) return;
    setBusy(`Exportando ${project.name}`);
    try {
      await invoke("export_project", { root, project: project.path, destination });
      setStatus(`“${project.name}” foi exportado com segurança.`);
      setStatusType("success");
    } catch (error) {
      setStatus(String(error));
      setStatusType("error");
    } finally {
      setBusy("");
    }
  }

  async function importPackage() {
    if (!root) return;
    const packagePath = await open({
      directory: false,
      multiple: false,
      title: "Importar projeto",
      filters: [{ name: "Pacote CapVault", extensions: ["capcutpkg", "zip"] }],
    });
    if (!packagePath) return;
    setBusy("Validando e importando projeto");
    try {
      const result = await invoke<ImportResult>("import_project", { root, package: packagePath });
      await refresh(root);
      setStatus(`“${result.projectName}” foi importado sem substituir projetos existentes.`);
      setStatusType("success");
    } catch (error) {
      setStatus(String(error));
      setStatusType("error");
    } finally {
      setBusy("");
    }
  }

  const visibleProjects = projects.filter((project) => project.name.toLocaleLowerCase("pt-BR").includes(deferredSearch.trim().toLocaleLowerCase("pt-BR")));
  const selected = projects.find((project) => project.path === selectedPath);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-mark">CV</div>
        <div><strong>CapVault</strong><span>Projetos CapCut protegidos e portáteis</span></div>
        <div className={`health ${root ? "online" : ""}`}><i />{root ? "Pasta conectada" : "Aguardando pasta"}</div>
      </header>

      <section className="hero">
        <div>
          <p className="eyebrow">BIBLIOTECA LOCAL</p>
          <h1>Seus projetos,<br /><em>sob seu controle.</em></h1>
          <p className="intro">Encontre, arquive e restaure projetos editáveis sem alterar os originais.</p>
        </div>
        <div className="hero-stat"><span>{projects.length.toString().padStart(2, "0")}</span><small>projetos<br />detectados</small></div>
      </section>

      <section className="root-panel">
        <div className="root-icon"><FolderIcon /></div>
        <div className="root-copy"><label>Pasta raiz do CapCut</label><p title={root}>{root || "Nenhuma pasta selecionada"}</p></div>
        <button className="button ghost" onClick={chooseRoot} disabled={!!busy}>Alterar pasta</button>
        <button className="button square" onClick={() => refresh()} disabled={!root || !!busy} aria-label="Atualizar projetos">↻</button>
      </section>

      <section className="workspace">
        <div className="list-pane">
          <div className="section-head">
            <div><p className="eyebrow">PROJETOS</p><h2>Biblioteca</h2></div>
            <div className="search"><span>⌕</span><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Buscar projeto" aria-label="Buscar projeto" /></div>
          </div>

          <div className="project-list">
            {!root && <div className="empty"><FolderIcon /><h3>Conecte sua biblioteca</h3><p>Selecione a pasta em que o CapCut guarda os projetos.</p><button className="button primary" onClick={chooseRoot}>Selecionar pasta</button></div>}
            {root && !busy && projects.length === 0 && <div className="empty"><FolderIcon /><h3>Nenhum projeto encontrado</h3><p>A pasta deve conter subpastas com os arquivos <code>draft_content.json</code> ou <code>draft_meta_info.json</code>.</p></div>}
            {root && projects.length > 0 && visibleProjects.length === 0 && <div className="empty compact"><h3>Nenhum resultado</h3><p>Tente buscar por outro nome.</p></div>}
            {visibleProjects.map((project) => (
              <button className={`project-row ${selectedPath === project.path ? "selected" : ""}`} key={project.path} onClick={() => setSelectedPath(project.path)}>
                <div className="project-monogram">{project.name.slice(0, 2).toUpperCase()}</div>
                <div className="project-main"><strong>{project.name}</strong><span>{project.fileCount.toLocaleString("pt-BR")} arquivos · {formatBytes(project.sizeBytes)}</span></div>
                <time>{new Date(project.modifiedAt).toLocaleDateString("pt-BR", { day: "2-digit", month: "short", year: "numeric" })}</time>
                <span className="chevron">›</span>
              </button>
            ))}
          </div>
        </div>

        <aside className="action-pane">
          <p className="eyebrow">AÇÕES SEGURAS</p>
          <h2>{selected ? selected.name : "Escolha um projeto"}</h2>
          <p>{selected ? "Crie um pacote portátil preservando toda a estrutura interna." : "Selecione um item da biblioteca para consultar os detalhes e exportá-lo."}</p>
          {selected && <dl><div><dt>Tamanho</dt><dd>{formatBytes(selected.sizeBytes)}</dd></div><div><dt>Arquivos</dt><dd>{selected.fileCount.toLocaleString("pt-BR")}</dd></div><div><dt>Modificado</dt><dd>{new Date(selected.modifiedAt).toLocaleDateString("pt-BR")}</dd></div></dl>}
          <button className="button primary full" disabled={!selected || !!busy} onClick={exportSelected}>Exportar projeto <span>→</span></button>
          <div className="divider"><span>OU</span></div>
          <button className="button secondary full" disabled={!root || !!busy} onClick={importPackage}>Importar pacote</button>
          <p className="safety">◆ Projetos existentes nunca são substituídos.</p>
        </aside>
      </section>

      <footer className={`statusbar ${statusType}`}><i />{busy || status}</footer>
      {busy && <div className="progress-line" />}
    </main>
  );
}

export default App;
