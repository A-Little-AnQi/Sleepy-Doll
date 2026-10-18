import { Button } from "@sleepy-doll/ui";
import { HelpIcon, Wordmark } from "@sleepy-doll/ui/icons";
import { useT } from "../../i18n";
import { ReleaseSettings } from "../../app/ReleaseSettings";

/** 设置「关于」页：产品名与软件更新都在这里，版本号只在更新区显示一次。 */
export function AboutPage() {
  const t = useT();
  return (
    <div className="settings-general settings-about-page">
      <h2>{t.settings.about}</h2>
      <div className="settings-about-brand">
        <Wordmark />
        <Button
          type="button"
          variant="subtle"
          onClick={() =>
            window.dispatchEvent(
              new CustomEvent("sleepy-doll:open-release-notes", {
                detail: "guide",
              }),
            )
          }
        >
          <HelpIcon className="button-icon" />
          {t.account.help}
        </Button>
      </div>
      <ReleaseSettings />
    </div>
  );
}
