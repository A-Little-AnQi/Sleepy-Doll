import { useId } from "react";
import { HelpIcon, CheckIcon } from "../icons";
import { useT } from "../../i18n";
import { MarkdownText } from "./Transcript";
import "./QuestionCard.css";

// Older questions used inline circled numbers. Preserve the wording and give
// each numbered item its own Markdown list row, without interpreting answers.
export function formatQuestion(question: string) {
  if ((question.match(/[①②③④⑤⑥⑦⑧⑨⑩]/g) ?? []).length < 2) return question;
  return question.replace(
    /\s*([①②③④⑤⑥⑦⑧⑨⑩])\s*/g,
    (_, marker: string) => `\n\n${"①②③④⑤⑥⑦⑧⑨⑩".indexOf(marker) + 1}. `,
  );
}

export function QuestionCard({
  question,
  submitted,
  inputId,
  id,
  onReply,
}: {
  question: string;
  submitted: boolean;
  inputId: string;
  id: string;
  onReply(): void;
}) {
  const t = useT();
  const headingId = useId();
  return (
    <section
      id={id}
      className="question-card"
      aria-labelledby={headingId}
      data-submitted={submitted}
    >
      <header className="question-card-header">
        {submitted ? (
          <CheckIcon className="button-icon" />
        ) : (
          <HelpIcon className="button-icon" />
        )}
        <h3 id={headingId}>
          {submitted ? t.chat.replySent : t.chat.answerToContinue}
        </h3>
        <span className="question-card-badge">
          {submitted ? t.chat.continuing : t.chat.waitingForReply}
        </span>
      </header>
      <div className="assistant-message question-card-body">
        <MarkdownText text={formatQuestion(question)} />
      </div>
      {!submitted && (
        <footer className="question-card-footer">
          <p>{t.chat.replyInstructions}</p>
          <button
            type="button"
            className="secondary-action"
            aria-controls={inputId}
            onClick={onReply}
          >
            {t.chat.writeReply}
          </button>
        </footer>
      )}
    </section>
  );
}
