// i18n-area: settings
/** The three places the language is chosen: the row at the top of Settings →
 * Appearance, the first card of New-user setup, and the one-time offer to an
 * existing user whose Windows is in Chinese. Each changes the language at once
 * through `choose`, which App.tsx gives (it applies the language and saves it). */
import { LANGUAGES, getLanguage, useLanguage, useT, windowsLanguage, type Language } from './index';
import './picker.css';

/** A globe in the sidebar icons' style. */
export function Globe({ size = 17 }: { size?: number }) {
  return <svg className="lang-globe" viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
    <circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3c2.6 2.6 3.9 5.6 3.9 9s-1.3 6.4-3.9 9c-2.6-2.6-3.9-5.6-3.9-9S9.4 5.6 12 3Z"/>
  </svg>;
}
const html = (id: Language) => LANGUAGES.find(item => item.id === id)?.html ?? id;
/** The word for "Language" in the other language, beside the label (a user who cannot read the
 * shown language still finds the row). */
function Other({ english, chinese }: { english: string; chinese: string }) {
  const shown = getLanguage();
  return <span className="lang-label-other" lang={shown === 'en' ? 'zh-Hans' : 'en'}>{shown === 'en' ? chinese : english}</span>;
}
/** What stays English in this version (the row and the card). */
function StaysEnglish() {
  const t = useT(), language = useLanguage();
  return <>
    {/* Only in another language than English, where it says something. */}
    {language !== 'en' && <p className="hint lang-hint">{t("Hero and item names follow the game's own names in the chosen language.")}</p>}
    <p className="hint lang-hint">{t('Error messages and their help, the build log and names the app makes itself stay in English in this version. File, bone and material names are never translated.')}</p>
  </>;
}

/** Settings → Appearance → Language. */
export function LanguageRow({ choose, disabled }: { choose: (language: Language) => void; disabled: boolean }) {
  const t = useT(), language = useLanguage();
  return <div className="lang-row">
    <div className="field lang-field">
      <label htmlFor="settings-language" className="lang-label"><Globe/>{t('Language')}<Other english="Language" chinese="语言"/></label>
      <select id="settings-language" disabled={disabled} value={language} onChange={event => choose(event.target.value as Language)}>
        {LANGUAGES.map(option => <option key={option.id} value={option.id} lang={option.html}>{option.name}</option>)}
      </select>
    </div>
    <p className="hint lang-hint">{t('At first start the app follows your Windows display language. Your choice is kept with the other Appearance preferences.')}</p>
    <StaysEnglish/>
  </div>;
}

/** New-user setup, first card: the languages as large buttons, the Windows language marked. */
export function LanguageCard({ choose, disabled }: { choose: (language: Language) => void; disabled: boolean }) {
  const t = useT(), language = useLanguage(), windows = windowsLanguage();
  return <section className="section prep-card lang-first" aria-labelledby="lang-first-title">
    <div className="section-heading"><h2 id="lang-first-title" className="lang-first-title"><Globe size={20}/>{t('Choose your language')}<Other english="Choose your language" chinese="选择语言"/></h2></div>
    <div className="lang-choices" role="radiogroup" aria-labelledby="lang-first-title">
      {LANGUAGES.map(option => <button key={option.id} type="button" role="radio" aria-checked={option.id === language} disabled={disabled} className={`lang-choice ${option.id === language ? 'active' : ''}`} onClick={() => choose(option.id)}>
        <span className="lang-choice-name" lang={option.html}>{option.id === language && '✓ '}{option.name}</span>
        {option.id === windows && <span className="lang-choice-tag">{t('Windows display language')}</span>}
      </button>)}
    </div>
    <p className="hint lang-hint">{t('You can change it any time in Settings → Appearance → Language.')}</p>
    <StaysEnglish/>
  </section>;
}

/** The one-time offer to an existing user on Chinese Windows, in both languages (it is
 * shown while the app is in English, to someone who may read only Chinese). */
export function LanguageOffer({ choose, disabled }: { choose: (language: Language) => void; disabled: boolean }) {
  return <div className="lang-offer" role="region" aria-label="Language · 语言">
    <span className="lang-offer-mark"><Globe size={18}/></span>
    <div className="lang-offer-text">
      <strong lang={html('zh-CN')}>Windows 的显示语言是简体中文。要把应用切换为简体中文吗？</strong>
      <span>Windows is set to Simplified Chinese. Switch the app to <span lang={html('zh-CN')}>简体中文</span>?</span>
    </div>
    <div className="lang-offer-actions">
      <button type="button" className="primary" lang={html('zh-CN')} disabled={disabled} onClick={() => choose('zh-CN')}>切换为简体中文</button>
      <button type="button" disabled={disabled} onClick={() => choose('en')}>Keep English · <span lang={html('zh-CN')}>保持英文</span></button>
    </div>
    <p className="lang-offer-foot">Asked once. Settings → Appearance → Language changes it any time. <span lang={html('zh-CN')}>只询问一次，之后可在 设置 → 外观 → 语言 中更改。</span></p>
  </div>;
}
