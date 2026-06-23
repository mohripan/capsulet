"use client";

import { FormEvent, useCallback, useEffect, useState } from "react";
import { Braces, FileCode2, PlugZap, Plus, RefreshCw, Save, Trash2 } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, PythonEditor } from "../components";
import {
  ContractField,
  TriggerPlugin,
  createTriggerPlugin,
  getErrorMessage,
  listTriggerPlugins
} from "../lib/api";

const starterTriggerScript = `import json
import os

config = json.loads(os.environ.get("CAPSULET_INPUT_JSON", "{}"))
threshold = int(config.get("threshold", 10))

# Replace this with your trigger check. Print one final JSON line:
# {"matched": true, "payload": {"reason": "threshold crossed"}}
print(json.dumps({
    "matched": False,
    "payload": {"threshold": threshold}
}))
`;

export default function TriggerPluginsPage() {
  const [plugins, setPlugins] = useState<TriggerPlugin[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pluginId, setPluginId] = useState("plugin_inventory_threshold");
  const [name, setName] = useState("Inventory threshold");
  const [runtimeImage, setRuntimeImage] = useState("python:3.12-slim");
  const [script, setScript] = useState(starterTriggerScript);
  const [fields, setFields] = useState<ContractField[]>([
    { name: "threshold", label: "Threshold", type: "number", required: true, default: 10 }
  ]);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listTriggerPlugins();
      setPlugins(response.trigger_plugins);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  function editPlugin(plugin: TriggerPlugin) {
    setSelectedId(plugin.id);
    setPluginId(plugin.id);
    setName(plugin.name);
    setRuntimeImage(plugin.runtime_image);
    setScript(plugin.python_script || starterTriggerScript);
    setFields((plugin.config_schema as { fields?: ContractField[] }).fields ?? []);
    setMessage(null);
    setError(null);
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSaving(true);
    setError(null);
    setMessage(null);
    try {
      const plugin = await createTriggerPlugin({
        id: pluginId,
        name,
        runtime_image: runtimeImage,
        python_script: script,
        config_schema: { fields }
      });
      setSelectedId(plugin.id);
      setMessage(`Saved trigger plugin ${plugin.name}.`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Trigger plugin registry"
        title="Author custom trigger Python scripts"
        description="Custom triggers evaluate external conditions and emit trigger events. They are separate from job definitions and run before a workflow starts."
      />
      <div className="pageToolbar">
        <button className="secondaryButton" type="button" onClick={() => void refresh()} disabled={isLoading}>
          <RefreshCw size={16} aria-hidden="true" />
          {isLoading ? "Refreshing" : "Refresh"}
        </button>
      </div>
      {error ? <div className="errorBox">{error}</div> : null}
      {message ? <div className="successBox">{message}</div> : null}

      <section className="contentGrid">
        <section className="panel span8 triggerPluginEditorPanel">
          <PanelTitle icon={FileCode2} title={selectedId ? "Edit trigger script" : "New trigger script"} action="Python" />
          <form className="formStack" onSubmit={save}>
            <div className="notebookMeta">
              <label><span>Plugin id</span><input value={pluginId} onChange={(event) => setPluginId(event.target.value)} required /></label>
              <label><span>Name</span><input value={name} onChange={(event) => setName(event.target.value)} required /></label>
              <label><span>Runtime image</span><input value={runtimeImage} onChange={(event) => setRuntimeImage(event.target.value)} required /></label>
            </div>
            <div className="triggerScriptContract">
              <Braces size={18} aria-hidden="true" />
              <span>Read config from <code>CAPSULET_INPUT_JSON</code>. Print one final JSON line: <code>{"{\"matched\": true, \"payload\": {}}"}</code>.</span>
            </div>
            <label>
              <span>Trigger Python script</span>
              <PythonEditor value={script} onChange={setScript} rows={22} />
            </label>
            <section className="triggerPluginFields">
              <div>
                <strong>Config fields</strong>
                <span>These fields appear when an automation uses this custom trigger.</span>
              </div>
              <div className="fieldContractList">
                {fields.map((field, index) => (
                  <div className="contractRow" key={`${field.name}-${index}`}>
                    <input value={field.name} onChange={(event) => setFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} />
                    <select value={field.type} onChange={(event) => setFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, type: event.target.value as ContractField["type"] } : item))}>
                      <option value="string">string</option>
                      <option value="number">number</option>
                      <option value="boolean">boolean</option>
                      <option value="textarea">textarea</option>
                      <option value="password">secret</option>
                    </select>
                    <button className="iconButton dangerButton" type="button" aria-label={`Remove ${field.name}`} onClick={() => setFields((current) => current.filter((_, itemIndex) => itemIndex !== index))}>
                      <Trash2 size={15} aria-hidden="true" />
                    </button>
                  </div>
                ))}
              </div>
              <button className="secondaryButton" type="button" onClick={() => setFields((current) => [...current, { name: "value", type: "string", required: true }])}>
                <Plus size={15} aria-hidden="true" />
                Field
              </button>
            </section>
            <button className="primaryAction fullWidthAction" disabled={isSaving || !script.trim()}>
              <Save size={16} aria-hidden="true" />
              {isSaving ? "Saving" : "Save trigger plugin"}
            </button>
          </form>
        </section>

        <section className="panel span4">
          <PanelTitle icon={PlugZap} title="Registry" action={`${plugins.length} plugins`} />
          <div className="resourceList">
            {!isLoading && plugins.length === 0 ? <div className="emptyState">No custom trigger plugins yet.</div> : null}
            {plugins.map((plugin) => (
              <button className={plugin.id === selectedId ? "workflowCatalogItem active" : "workflowCatalogItem"} type="button" key={plugin.id} onClick={() => editPlugin(plugin)}>
                <span className="workflowCatalogTitle"><strong>{plugin.name}</strong><small>{plugin.runtime_image}</small></span>
                <span className="workflowCatalogMeta">{plugin.id}</span>
                <span>{((plugin.config_schema as { fields?: ContractField[] }).fields ?? []).length} config fields</span>
              </button>
            ))}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
