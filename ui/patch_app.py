import re
import os

filepath = r'c:\Users\kacc2\OneDrive\Desktop\screentime\ui\src\App.tsx'
with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# I will add lucide-react icons and replace the emojis in the metrics
content = content.replace('import { formatDayLabel', 'import { Monitor, Target, PauseCircle, Shield } from "lucide-react";\nimport { formatDayLabel')

content = re.sub(r'icon: <span[^>]+>📺</span>', 'icon: <Monitor size={20} color="var(--color-primary)" />', content)
content = re.sub(r'icon: <span[^>]+>🎯</span>', 'icon: <Target size={20} color="var(--color-primary)" />', content)
content = re.sub(r'icon: <span[^>]+>⏸️</span>', 'icon: <PauseCircle size={20} color="var(--color-danger)" />', content)
content = re.sub(r'icon: <span[^>]+>🛡️</span>', 'icon: <Shield size={20} color="var(--color-primary)" />', content)

advanced_settings = '''
                <section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>Advanced Parameters</h2>
                    <div className="card-subtitle">Fine-tune system thresholds and enforcement behaviour.</div>
                  </header>
                  <div className="card-body">
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Anti-impulse Cooldown (Hours)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Time delay before a relaxed limit takes effect. Tightening applies instantly.
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        value={statusInfo?.limit_cooldown_hours?.toString() ?? "24"}
                        onChange={(e) => actions.setSetting("limit_cooldown_hours", e.target.value)}
                      />
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
                      <div>
                        <div className="form-label">Idle Threshold (Seconds)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Seconds without input before usage stops accruing.
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        value={statusInfo?.idle_threshold_secs?.toString() ?? "60"}
                        onChange={(e) => actions.setSetting("idle_threshold_secs", e.target.value)}
                      />
                    </div>
                    <div className="form-row" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div className="form-label">Day Start Offset (Minutes)</div>
                        <div className="form-hint" style={{ marginTop: 0 }}>
                          Minutes after local midnight at which daily budgets reset (0 = midnight).
                        </div>
                      </div>
                      <input
                        type="number"
                        className="input"
                        style={{ width: '80px' }}
                        value={statusInfo?.day_start_minutes?.toString() ?? "0"}
                        onChange={(e) => actions.setSetting("day_start_minutes", e.target.value)}
                      />
                    </div>
                  </div>
                </section>
'''

target_string = '''<section className="card" style={{ marginTop: 16 }}>
                  <header className="card-header">
                    <h2>Security</h2>'''

content = content.replace(target_string, advanced_settings + target_string)

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(content)

print("App.tsx updated")
