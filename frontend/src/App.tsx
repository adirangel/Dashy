import { useMemo, useState } from "react";
import { Check, ChevronLeft, Flame, Plus, Sparkles, X } from "lucide-react";
import { activity } from "./activity";
import type { Todo, Usage } from "./types";

const initialTodos: Todo[] = [
  { id: "1", title: "לסיים את המצגת השבועית", due: "היום", completed: false },
  { id: "2", title: "לעבור על משימות הפרויקט", due: "מחר", completed: false },
  { id: "3", title: "לשלוח סיכום פגישה", due: "יום א׳", completed: true },
];
const usage: Usage[] = [
  { name: "Claude", used: 32, limit: 50, tone: "orange" },
  { name: "Codex", used: 68, limit: 100, tone: "green" },
];

function App() {
  const [todos, setTodos] = useState(initialTodos);
  const [adding, setAdding] = useState(false);
  const [title, setTitle] = useState("");
  const completed = useMemo(() => todos.filter((todo) => todo.completed).length, [todos]);
  const toggle = (id: string) => setTodos((items) => items.map((item) => item.id === id ? { ...item, completed: !item.completed } : item));
  const addTodo = () => {
    if (!title.trim()) return;
    setTodos((items) => [{ id: crypto.randomUUID(), title: title.trim(), due: "היום", completed: false }, ...items]);
    setTitle(""); setAdding(false);
  };

  return <main className="shell">
    <header className="topbar" data-tauri-drag-region>
      <div className="brand"><span className="brand-mark"><Sparkles size={14} /></span><span>Dashy</span></div>
      <button className="icon-button" aria-label="סגירה"><X size={16} /></button>
    </header>

    <section className="hero card">
      <div className="streak-copy"><span className="eyebrow">הרצף שלך</span><strong><Flame size={17} fill="currentColor" /> 12 ימים</strong><small>היום ממשיכים קדימה</small></div>
      <div className="heatmap" dir="ltr" aria-label="מפת פעילות של 12 שבועות">
        <div className="months"><span>יוני</span><span>יולי</span><span>אוג׳</span></div>
        <div className="squares">{activity.map((level, index) => <i key={index} data-level={level} />)}</div>
      </div>
    </section>

    <section className="usage-grid">
      {usage.map((metric) => { const percentage = Math.round(metric.used / metric.limit * 100); return <article className={`usage card ${metric.tone}`} key={metric.name}>
        <div className="usage-heading"><div><b>{metric.name}</b><small>מכסה חודשית</small></div><strong>{percentage}%</strong></div>
        <div className="progress"><span style={{ width: `${percentage}%` }} /></div>
        <div className="usage-foot"><span>{metric.used} מתוך {metric.limit}</span><span>נותרו {metric.limit - metric.used}</span></div>
      </article>; })}
    </section>

    <section className="todos card">
      <div className="section-heading"><div><span className="eyebrow">המשימות שלי</span><h2>מה עושים היום?</h2></div><button className="add-button" onClick={() => setAdding(true)}><Plus size={16} /> משימה</button></div>
      {adding && <form className="add-form" onSubmit={(event) => { event.preventDefault(); addTodo(); }}><label className="sr-only" htmlFor="todo-title">שם המשימה</label><input id="todo-title" autoFocus value={title} onChange={(event) => setTitle(event.target.value)} placeholder="שם המשימה..." /><button>הוספה</button></form>}
      <div className="todo-list">{todos.map((todo) => <button className={`todo ${todo.completed ? "done" : ""}`} key={todo.id} onClick={() => toggle(todo.id)}>
        <span className="checkbox">{todo.completed && <Check size={13} strokeWidth={3} />}</span><span className="todo-title">{todo.title}</span><span className={`due ${todo.due === "היום" ? "today" : ""}`}>{todo.due}</span><ChevronLeft size={15} className="chevron" />
      </button>)}</div>
      <div className="todo-summary"><span>{completed}/{todos.length} הושלמו</span><div className="mini-progress"><i style={{ width: `${todos.length ? completed / todos.length * 100 : 0}%` }} /></div></div>
    </section>

  </main>;
}

export default App;
