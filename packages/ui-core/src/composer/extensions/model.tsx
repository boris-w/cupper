// "{模型} {强度} ⌄" (design 5.7.4): the model and the reasoning effort in one
// button, each a group of its menu. An error's "换模型" opens it too. The
// agent's names stay as they are: a long one is cut short, whole in the
// tooltip (T272).

import { useStore } from "zustand";

import { useT } from "../../app/providers";
import { ChevronDownIcon } from "../../components/icons";
import { Menu, MenuSeparator } from "../../components/Menu";
import { useUi } from "../../state/provider";
import { BarButton, Choices, WithOptions, type Options } from "../config";
import type { ComposerExtension } from "../extensions";

function Model({ options }: { options: Options }) {
  const t = useT();
  const ui = useUi();
  const open = useStore(ui, (state) => state.menu === "model");
  const model = options.get("model");
  const effort = options.get("reasoning_effort");
  if (!model.option && !effort.option) {
    return null;
  }
  const label = [model.value?.name, effort.value?.name].filter(Boolean).join(" ");
  return (
    <Menu
      open={open}
      onOpenChange={(opened) => ui.getState().openMenu(opened ? "model" : null)}
      trigger={
        <BarButton label={t("composer:model")} tooltip={label}>
          <span className="max-w-48 truncate">{label}</span>
          <ChevronDownIcon />
        </BarButton>
      }
    >
      {model.option && (
        <Choices
          title={t("composer:model_title")}
          option={model.option}
          value={model.value}
          pick={(picked) => model.option && options.set(model.option, picked)}
        />
      )}
      {model.option && effort.option && <MenuSeparator />}
      {effort.option && (
        <Choices
          title={t("composer:effort_title")}
          option={effort.option}
          value={effort.value}
          pick={(picked) => effort.option && options.set(effort.option, picked)}
        />
      )}
    </Menu>
  );
}

export default {
  side: "right",
  order: 20,
  Component: (props) => (
    <WithOptions {...props}>{(options) => <Model options={options} />}</WithOptions>
  ),
} satisfies ComposerExtension;
