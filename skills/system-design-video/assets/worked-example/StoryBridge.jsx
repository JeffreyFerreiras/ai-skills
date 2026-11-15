function StoryBridge({ progress }) {
  const p = Math.min(1, Math.max(0, progress ?? 0));
  const hang = Math.min(1, p / 0.3);
  const signY = -60 * (1 - hang);
  const repeat = p < 0.35 ? 0 : Math.min(1, (p - 0.35) / 0.25);
  const wobble = p < 0.4 || p > 0.75 ? 0 : Math.sin((p - 0.4) * Math.PI * 4) * 3;
  const hold = p >= 0.7;
  const armX = 540 + 180 * (1 - hang);
  return (
    <g>
      <title>Shopkeeper hangs the CLOSED sign; a repeated instruction leaves it unchanged</title>
      <rect x="300" y="480" width="480" height="80" rx="20" fill="#1d3c4d" stroke="#84d1bc" strokeWidth="4" />
      <rect x="340" y="560" width="400" height="620" rx="12" fill="#0b1c26" stroke="#799cab" strokeWidth="6" />
      <rect x="360" y="600" width="360" height="540" rx="8" fill="#12303f" />
      <line x1="430" y1="560" x2="430" y2="700" stroke="#799cab" strokeWidth="5" />
      <line x1="650" y1="560" x2="650" y2="700" stroke="#799cab" strokeWidth="5" />
      <g transform={`translate(540 ${760 + signY}) rotate(${wobble})`}>
        <rect x="-160" y="-70" width="320" height="140" rx="14" fill="#f5f0dd" stroke="#f6bf66" strokeWidth="6" />
        <text textAnchor="middle" y="22" fill="#10202d" fontFamily="Arial" fontSize="64" fontWeight="700">CLOSED</text>
      </g>
      <circle cx="220" cy="700" r="58" fill="#84d1bc" />
      <rect x="162" y="758" width="116" height="220" rx="40" fill="#84d1bc" />
      <line x1="220" y1="820" x2={armX} y2={730 + signY} stroke="#84d1bc" strokeWidth="22" strokeLinecap="round" />
      <g opacity={repeat}>
        <rect x="740" y="640" width="240" height="150" rx="18" fill="#f5f0dd" />
        <text x="762" y="700" fill="#10202d" fontFamily="Arial" fontSize="36">Close again?</text>
        <text x="762" y="748" fill="#10202d" fontFamily="Arial" fontSize="36">Already shut.</text>
      </g>
      {hold && (
        <g>
          <circle cx="700" cy="1060" r="44" fill="none" stroke="#84d1bc" strokeWidth="8" />
          <path d="M 678 1060 l 16 16 l 30 -34" fill="none" stroke="#84d1bc" strokeWidth="8" strokeLinecap="round" />
        </g>
      )}
    </g>
  );
}

export default { "shop-sign": StoryBridge };
