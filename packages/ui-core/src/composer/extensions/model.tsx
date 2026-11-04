// "{模型} {强度} ⌄" (design 5.7.4): the model and the reasoning effort in one
// button, each a group of its menu. An error's "换模型" opens it too.

import type { SessionView } from "@cupper/host-client";
import { useStore } from "zustand";

import { useT } from "../../app/providers";
import { ChevronDownIcon } from "../../components/icons";
import { Menu, MenuSeparator } from "../../components/Menu";
import { useUi } from "../../state/provider";
import { BarButton, Choices, useOption, useSetOption, WithSession } from "../config";
import type { ComposerExtension } from "../extensions";

function Model({ view, sessionId }: { view: SessionView; sessionId: string }) {
  const t = useT();
  const ui = useUi();
  const open = useStore(ui, (state) => state.menu === "model");
  const model = useOption(view, "model");
  const effort = useOption(view, "reasoning_effort");
  const set = useSetOption(sessionId);
  if (!model.option && !effort.option) {
    return null;
  }
  const label = [model.value?.name, effort.value?.name].filter(Boolean).join(" ");
  return (
    <Menu
      open={open}
      onOpenChange={(opened) => ui.getState().openMenu(opened ? "model" : null)}
      trigger={
        <BarButton label={t("composer:model")}>
          {label}
          <ChevronDownIcon />
        </BarButton>
      }
    >
      {model.option && (
        <Choices
          title={t("composer:model_title")}
          option={model.option}
          value={model.value}
          pick={(picked) => model.option && void set(model.option, picked)}
        />
      )}
      {model.option && effort.option && <MenuSeparator />}
      {effort.option && (
        <Choices
          title={t("composer:effort_title")}
          option={effort.option}
          value={effort.value}
          pick={(picked) => effort.option && void set(effort.option, picked)}
        />
      )}
    </Menu>
  );
}

export default {
  side: "right",
  order: 20,
  Component: ({ sessionId }) => (
    <WithSession sessionId={sessionId}>
      {(view, id) => <Model view={view} sessionId={id} />}
    </WithSession>
  ),
} satisfies ComposerExtension;
