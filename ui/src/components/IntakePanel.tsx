import React, { useState } from 'react';
import styles from './IntakePanel.module.css';

interface Props {
  questions: string[];
  onSubmit: (answers: string[]) => void;
}

export function IntakePanel({ questions, onSubmit }: Props) {
  const [answers, setAnswers] = useState<string[]>(questions.map(() => ''));

  const handleChange = (i: number, val: string) => {
    setAnswers((prev) => {
      const next = [...prev];
      next[i] = val;
      return next;
    });
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSubmit(answers);
  };

  const allAnswered = answers.every((a) => a.trim().length > 0);

  return (
    <form className={styles.panel} onSubmit={handleSubmit} aria-label="Intake questions">
      <div className={styles.header}>
        <span className={styles.label}>Intake questions</span>
        <span className={styles.hint}>Answer all fields to continue</span>
      </div>
      <div className={styles.questions}>
        {questions.map((q, i) => (
          <div key={i} className={styles.field}>
            <label className={styles.qLabel} htmlFor={`intake-q-${i}`}>
              {i + 1}. {q}
            </label>
            <textarea
              id={`intake-q-${i}`}
              className={styles.textarea}
              value={answers[i] ?? ''}
              onChange={(e) => handleChange(i, e.target.value)}
              rows={2}
              placeholder="Your answer…"
              required
            />
          </div>
        ))}
      </div>
      <div className={styles.actions}>
        <button
          type="submit"
          className={styles.submitBtn}
          disabled={!allAnswered}
        >
          Submit answers
        </button>
      </div>
    </form>
  );
}
