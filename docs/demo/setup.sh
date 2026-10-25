# Sourced by demo.tape (hidden): a small demo project and a clean prompt.
# Expects TACKLINE_BIN to be a folder with the `tack` binary.
set -e
demo="$(mktemp -d)/shop"
mkdir -p "$demo/src" "$demo/scripts" "$demo/web"
cd "$demo"
cat > src/matcher.rs <<'RS'
/// Items whose name contains the query.
pub fn matches(query: &str, items: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if item.contains(query) {
            out.push(item.clone());
        }
    }
    out
}
RS
cat > scripts/score.py <<'PY'
def score(values):
    total = sum(v * v for v in values)
    return total / len(values)
PY
cat > web/app.ts <<'TS'
export function count(items: string[]): number {
  return items.length;
}
TS
printf '.tackline/\n' > .gitignore
git init -q && git add . && git -c user.name=demo -c user.email=demo@example.com -c commit.gpgsign=false commit -qm init
printf '\nexport const VERSION = 2;\n' >> web/app.ts
export PATH="$TACKLINE_BIN:$PATH"
export PS1='\[\e[36m\]shop\[\e[0m\] $ '
set +e
clear
