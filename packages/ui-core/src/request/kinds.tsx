// The three kinds of request on the panel (design 5.7.4): a permission and a
// plan confirmation answer with one of their options, a question with its
// fields. Agent text is escaped and only ever a text node.

import { escapeInvisible } from "@cupper/i18n";
import type {
  AnswerValue,
  Content,
  InteractionOption,
  InteractionOutcome,
  InteractionRequested,
  QuestionField,
} from "@cupper/protocol";
import { useState, type MutableRefObject, type ReactNode, type Ref } from "react";

import { useT } from "../app/providers";
import { initialSelection, isEnabled, isPrivileged, move, type PanelKey } from "./logic";

export interface KindProps {
  requested: InteractionRequested;
  answer(outcome: InteractionOutcome): void;
  /** A privileged option goes to the native dialog (RP-7). */
  confirm(option: InteractionOption): void;
  /** Wraps a click: one whose mousedown came before the panel was ready does nothing (RP-2). */
  click(fn: () => void): () => void;
  /** Set by the kind: what it does with a panel key once the panel takes keys. */
  keys: MutableRefObject<(key: PanelKey) => void>;
  listRef: Ref<HTMLUListElement>;
}

export const BUTTON =
  "rounded-lg px-2.5 py-1 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
const KEYCAP = "ml-1.5 rounded-xs border border-current px-1 text-xs opacity-60";

export const text = (content?: Content | null) =>
  content?.type === "text" && "text" in content ? escapeInvisible(String(content.text)) : undefined;

interface Item {
  label: string;
  notes: string[];
  disabled?: boolean;
  checked?: boolean;
}

/** Numbered rows: the number is the item's position from 1, never reordered (RP-6). */
function NumberedList(props: {
  items: Item[];
  selected: number;
  pick: (index: number) => void;
  listRef: Ref<HTMLUListElement>;
  click: KindProps["click"];
}) {
  return (
    <ul
      ref={props.listRef}
      tabIndex={-1}
      role="listbox"
      className="flex flex-col gap-0.5 px-2 outline-none"
    >
      {props.items.map((item, index) => (
        <li
          key={index}
          role="option"
          aria-selected={index === props.selected}
          aria-disabled={item.disabled || undefined}
          aria-checked={item.checked}
          onClick={props.click(() => !item.disabled && props.pick(index))}
          className={`flex items-start gap-2 rounded-lg px-2 py-1.5 text-sm aria-disabled:opacity-50 ${index === props.selected ? "bg-hover" : ""}`}
        >
          <span className="w-4 shrink-0 text-fg-tertiary">
            {props.items.length > 1 ? `${index + 1}.` : ""}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block font-medium">{item.label}</span>
            {item.notes.map((note) => (
              <span key={note} className="block text-fg-tertiary">
                {note}
              </span>
            ))}
          </span>
          {index === props.selected && <span className="text-fg-tertiary">↑↓</span>}
        </li>
      ))}
    </ul>
  );
}

function Footer(props: {
  children?: ReactNode;
  skip: () => void;
  submit: () => void;
  canSubmit: boolean;
  click: KindProps["click"];
}) {
  const t = useT();
  return (
    <div className="flex items-center justify-end gap-1 px-3 pt-2 pb-3 text-sm">
      {props.children}
      <button type="button" onClick={props.click(props.skip)} className={BUTTON}>
        {t("request:skip")}
      </button>
      <button
        type="button"
        disabled={!props.canSubmit}
        onClick={props.click(props.submit)}
        className={`${BUTTON} bg-btn-primary text-btn-primary-fg hover:bg-btn-primary`}
      >
        {t("request:submit")}
        <span className={KEYCAP}>{t("request:key_enter")}</span>
      </button>
    </div>
  );
}

/** A permission or a plan confirmation: the content above, the options below. */
export function OptionsKind(
  props: KindProps & {
    options: InteractionOption[];
    truncated: boolean;
    skip: InteractionOutcome;
    children: ReactNode;
  },
) {
  const t = useT();
  const { options, truncated } = props;
  const [selected, setSelected] = useState(() => initialSelection(options, truncated));
  const choose = (index: number, byKey: boolean) => {
    const option = options[index];
    // Unknown effect: no number, no Enter; a click leads to the native dialog (RP-6).
    if (!option || !isEnabled(option, truncated) || (byKey && option.effect.scope === "unknown")) {
      return;
    }
    setSelected(index);
    if (isPrivileged(option)) {
      props.confirm(option);
    } else {
      props.answer({ type: "selected", option_id: option.option_id });
    }
  };
  props.keys.current = (key) => {
    if (key.type === "digit") choose(key.index, true);
    if (key.type === "up" || key.type === "down")
      setSelected(move(options, selected, key.type === "up" ? -1 : 1, truncated));
    if (key.type === "submit" || key.type === "cmd_submit") choose(selected, true);
  };
  const notes = (option: InteractionOption) =>
    [
      option.effect.scope === "unknown" && t("request:unknown_effect"),
      option.effect.scope !== "unknown" && isPrivileged(option) && t("request:needs_confirm"),
      option.effect.persistent && t("request:persistent"),
      option.effect.ends_turn && t("request:ends_turn"),
    ].filter((note): note is string => Boolean(note));
  return (
    <>
      {props.children}
      {truncated && <p className="px-4 pb-2 text-sm text-warning">{t("request:truncated")}</p>}
      <NumberedList
        items={options.map((option) => ({
          label: escapeInvisible(option.name),
          notes: notes(option),
          disabled: !isEnabled(option, truncated),
        }))}
        selected={selected}
        pick={(index) => choose(index, false)}
        listRef={props.listRef}
        click={props.click}
      />
      <Footer
        skip={() => props.answer(props.skip)}
        submit={() => choose(selected, false)}
        canSubmit={selected >= 0}
        click={props.click}
      />
    </>
  );
}

/** The command in a "终端" box, collapsed to a few lines until expanded. */
export function Terminal({ command }: { command: string }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="mx-4 mb-2 overflow-hidden rounded-lg border border-border bg-surface-code text-code">
      <div className="flex items-center justify-between px-3 py-1 text-xs text-fg-tertiary">
        {t("request:terminal")}
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="rounded-md px-1 hover:bg-hover"
        >
          {t(open ? "request:collapse" : "request:expand")}
        </button>
      </div>
      <pre
        className={`overflow-x-auto px-3 pb-2 font-mono whitespace-pre-wrap ${open ? "" : "max-h-24 overflow-y-hidden"}`}
      >
        {command}
      </pre>
    </div>
  );
}

const valueLabel = (value: AnswerValue | undefined) =>
  Array.isArray(value) ? value : value === undefined ? [] : [String(value)];

/**
 * A question, one field at a time (design 5.7.4): ← → move between fields;
 * a choice by number or Enter answers the field and moves on; on the last
 * field the answers go. Skip leaves a field that is not required empty.
 */
export function QuestionKind(props: KindProps & { fields: QuestionField[] }) {
  const t = useT();
  const { fields } = props;
  const [at, setAt] = useState(0);
  const [values, setValues] = useState<Record<string, AnswerValue>>({});
  const [selected, setSelected] = useState(0);
  const [custom, setCustom] = useState("");
  const field = fields[at];
  if (!field) {
    return null;
  }
  const choices =
    field.type === "boolean"
      ? [
          { label: t("request:yes"), value: "true" },
          { label: t("request:no"), value: "false" },
        ]
      : (field.choices ?? []);
  const go = (to: number) => {
    setAt(Math.max(0, Math.min(fields.length - 1, to)));
    setSelected(0);
    setCustom("");
  };
  const finish = (next: Record<string, AnswerValue>) => {
    if (at < fields.length - 1) {
      return go(at + 1);
    }
    props.answer(
      Object.keys(next).length > 0 ? { type: "answer", values: next } : { type: "dismissed" },
    );
  };
  const record = (value: AnswerValue | undefined) => {
    const next = { ...values };
    if (value !== undefined) next[field.field_id] = value;
    setValues(next);
    finish(next);
  };
  const choose = (index: number) => {
    const choice = choices[index];
    if (!choice) return;
    if (field.type === "multi_choice") {
      const current = valueLabel(values[field.field_id]);
      const toggled = current.includes(choice.value)
        ? current.filter((v) => v !== choice.value)
        : [...current, choice.value];
      setValues({ ...values, [field.field_id]: toggled });
      setSelected(index);
      return;
    }
    record(field.type === "boolean" ? choice.value === "true" : choice.value);
  };
  const submitField = () => (field.type === "multi_choice" ? finish(values) : choose(selected));
  props.keys.current = (key) => {
    if (key.type === "digit") choose(key.index);
    if (key.type === "up" || key.type === "down")
      setSelected(
        Math.max(0, Math.min(choices.length - 1, selected + (key.type === "up" ? -1 : 1))),
      );
    if (key.type === "left" || key.type === "right") go(at + (key.type === "left" ? -1 : 1));
    if (key.type === "submit" || key.type === "cmd_submit") submitField();
  };
  const freeText = choices.length === 0 || field.allow_custom;
  return (
    <>
      <p className="px-4 pb-2 text-sm font-medium">{escapeInvisible(field.label)}</p>
      {choices.length > 0 && (
        <NumberedList
          items={choices.map((choice) => ({
            label: escapeInvisible(choice.label),
            notes: choice.description ? [escapeInvisible(choice.description)] : [],
            checked:
              field.type === "multi_choice"
                ? valueLabel(values[field.field_id]).includes(choice.value)
                : undefined,
          }))}
          selected={selected}
          pick={choose}
          listRef={props.listRef}
          click={props.click}
        />
      )}
      {freeText && (
        <input
          value={custom}
          placeholder={t("request:custom")}
          aria-label={t("request:custom")}
          onChange={(event) => setCustom(event.target.value)}
          onKeyDown={(event) =>
            event.key === "Enter" &&
            !event.nativeEvent.isComposing &&
            custom.trim() &&
            record(field.type === "number" ? Number(custom) : custom)
          }
          className="mx-4 mt-1 rounded-lg border border-border bg-transparent px-2 py-1 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        />
      )}
      <Footer
        skip={() => (field.required ? undefined : record(undefined))}
        submit={submitField}
        canSubmit={choices.length > 0 || custom.trim() !== ""}
        click={props.click}
      >
        {fields.length > 1 && (
          <>
            <button
              type="button"
              disabled={at === 0}
              onClick={props.click(() => go(at - 1))}
              className={BUTTON}
            >
              {t("request:prev")}
            </button>
            <button
              type="button"
              disabled={at === fields.length - 1}
              onClick={props.click(() => go(at + 1))}
              className={BUTTON}
            >
              {t("request:next")}
            </button>
          </>
        )}
      </Footer>
    </>
  );
}
