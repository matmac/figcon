export type Page = "setup" | "clients";
export function Navigation({ page, onNavigate, installing }: { page: Page; onNavigate(page: Page): void; installing: boolean }) {
  return <nav className="app-navigation" aria-label="Main navigation"><button type="button" aria-current={page === "setup" ? "page" : undefined} onClick={() => onNavigate("setup")}>Setup{installing && <span className="navigation-live"> · In progress</span>}</button><button type="button" aria-current={page === "clients" ? "page" : undefined} onClick={() => onNavigate("clients")}>AI clients</button></nav>;
}
