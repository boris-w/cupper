// The permission button (design 5.7.4, CP-4): a shield and the mode's short
// name; its menu lists the agent's modes, with what each looser one means.

import { useT } from "../../app/providers";
import { ChevronDownIcon, ShieldIcon } from "../../components/icons";
import { Menu } from "../../components/Menu";
import { BarButton, Choices, fullAccess, WithOptions, type Options } from "../config";
import type { ComposerExtension } from "../extensions";

function Permission({ options }: { options: Options }) {
  const t = useT();
  const { option, value } = options.get("permission_mode");
  if (!option) {
    return null;
  }
  const hint = (v: typeof value) =>
    fullAccess(v)
      ? t("composer:tier_full_access")
      : v?.risk === "elevated"
        ? t("composer:tier_elevated")
        : undefined;
  return (
    <Menu
      align="start"
      trigger={
        <BarButton label={t("composer:permission")}>
          <ShieldIcon />
          {value?.name}
          <ChevronDownIcon />
        </BarButton>
      }
    >
      <Choices
        title={t("composer:permission_title")}
        option={option}
        value={value}
        pick={(picked) => options.set(option, picked)}
        hint={hint}
      />
    </Menu>
  );
}

export default {
  side: "left",
  order: 20,
  Component: (props) => (
    <WithOptions {...props}>{(options) => <Permission options={options} />}</WithOptions>
  ),
} satisfies ComposerExtension;
