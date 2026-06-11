// The single reusable torch SVG — geometry lifted verbatim from
// torch-identity.html. The geometry is inlined per instance (NOT via
// <use href>): CSS class selectors cannot reach inside a <use> shadow
// tree, so state classes like .t-spent .flame would silently never apply.

import { useId } from 'react';

interface TorchProps {
  width?: number;
  height?: number;
  state: 'unlit' | 'lit' | 'spent' | 'guttered';
}

const stateClass: Record<string, string> = {
  unlit: 't-unlit',
  lit: 't-lit',
  spent: 't-spent',
  guttered: 't-gutter',
};

export function Torch({ width = 22, height = 33, state }: TorchProps) {
  // each instance needs its own gradient id — ids are document-global
  const gradientId = useId();
  return (
    <span className={stateClass[state] ?? 't-unlit'}>
      <svg
        className="torch"
        width={width}
        height={height}
        viewBox="0 0 64 96"
        aria-label={`Stage ${state}`}
        role="img"
      >
        <defs>
          <radialGradient id={gradientId} cx="50%" cy="45%" r="50%">
            <stop offset="0%" stopColor="#E8A33D" stopOpacity="0.5" />
            <stop offset="100%" stopColor="#E8A33D" stopOpacity="0" />
          </radialGradient>
        </defs>
        <circle className="glow" cx="32" cy="26" r="22" fill={`url(#${gradientId})`} />
        <g className="flame">
          <path className="f-out"  fill="#E8A33D" d="M32 5 C40 13 45.5 21 44 29.5 C42.5 37.5 37 41 32 41 C27 41 21.5 37.5 20 29.5 C18.5 21 24 13 32 5 Z" />
          <path className="f-mid"  fill="#F2C063" d="M32 14 C37 19.5 40 24.5 39 29.8 C38 34.8 35 37.5 32 37.5 C29 37.5 26 34.8 25 29.8 C24 24.5 27 19.5 32 14 Z" />
          <path className="f-core" fill="#FBE9C0" d="M32 22 C34.8 25.2 36.3 28 35.6 31 C35 33.8 33.6 35.2 32 35.2 C30.4 35.2 29 33.8 28.4 31 C27.7 28 29.2 25.2 32 22 Z" />
        </g>
        <path className="char" fill="#221C16" d="M27 36 L29 32.5 L31 35 L33 31.8 L35 34.6 L37 36 L37 40 L27 40 Z" />
        <circle className="emberdot" cx="33.4" cy="36.8" r="1.5" fill="#D9824A" />
        <rect x="26.6" y="39" width="10.8" height="9.5" rx="2" fill="#3A3128" />
        <rect x="26.6" y="42.6" width="10.8" height="1.4" fill="#2A231C" />
        <path d="M29.4 48.5 L34.6 48.5 L33.2 88 C33.2 89.3 30.8 89.3 30.8 88 Z" fill="#4A3A28" />
      </svg>
    </span>
  );
}
