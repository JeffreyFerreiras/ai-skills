import React from 'react';
import {AbsoluteFill, Audio, Composition, Sequence, registerRoot, staticFile, useCurrentFrame} from 'remotion';
import spec from '../video-spec.json';
import {compositionFrames, frameAt, stateAt} from './timeline.mjs';

function Explainer({spec}) {
  const frame = useCurrentFrame();
  const fps = spec.profile.fps;
  const time = frame / fps;
  const scene = spec.scenes.find((item) => time >= item.start && time < item.end);
  const {states, packets} = stateAt(spec, frame);
  const positions = Object.fromEntries(spec.nodes.map((node, index) => [node.id,
    node.position ?? {x: 540, y: 560 + index * 720 / Math.max(1, spec.nodes.length - 1)}]));
  const route = (edge) => {
    const a = positions[edge.from], b = positions[edge.to];
    const length = Math.hypot(b.x - a.x, b.y - a.y);
    const ux = (b.x - a.x) / length, uy = (b.y - a.y) / length;
    const shift = 26;
    return {x1: a.x + ux * 100 - uy * shift, y1: a.y + uy * 100 + ux * shift,
      x2: b.x - ux * 100 - uy * shift, y2: b.y - uy * 100 + ux * shift};
  };
  return <AbsoluteFill style={{backgroundColor: '#10202d', color: '#f5f0dd'}}>
    <svg viewBox="0 0 1080 1920" width="100%" height="100%" role="img" aria-label={spec.question}>
      <defs><marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="9" markerHeight="9" orient="auto-start-reverse">
        <path d="M 0 0 L 10 5 L 0 10 z" fill="#799cab" /></marker></defs>
      <foreignObject x="80" y="140" width="920" height="280"><div style={{fontFamily: 'Arial, sans-serif', fontSize: 62, lineHeight: 1.15, fontWeight: 700}}>{spec.question}</div></foreignObject>
      {spec.edges.map((edge) => <line key={edge.id} {...route(edge)} stroke="#799cab" strokeWidth="7" markerEnd="url(#arrow)" />)}
      {spec.nodes.map((node) => <g key={node.id} transform={`translate(${positions[node.id].x}, ${positions[node.id].y})`}>
        <rect x="-230" y="-90" width="460" height="180" rx="24" fill="#1d3c4d" stroke="#84d1bc" strokeWidth="4" />
        <text textAnchor="middle" y="-12" fill="#f5f0dd" fontFamily="Arial" fontSize="48">{node.label}</text>
        <text textAnchor="middle" y="48" fill="#84d1bc" fontFamily="Arial" fontSize="30">{states[node.id] ?? 'Ready'}</text>
      </g>)}
      {packets.map((packet) => {
        const line = route(spec.edges.find((edge) => edge.id === packet.edge_id));
        const x = line.x1 + (line.x2 - line.x1) * packet.progress;
        const y = line.y1 + (line.y2 - line.y1) * packet.progress;
        return <g key={packet.id}><circle cx={x} cy={y} r="18" fill="#f6bf66" />
          <text x={x + 35} y={y + 10} fill="#f6bf66" fontFamily="Arial" fontSize="32">{packet.label ?? 'Message'}</text></g>;
      })}
      <foreignObject x="100" y="1480" width="880" height="280"><div style={{fontFamily: 'Arial, sans-serif', fontSize: 44, lineHeight: 1.25}}>
        {spec.presentation?.burn_captions ? spec.captions?.find((cue) => time >= cue.start && time < cue.end)?.text : scene?.caption}
      </div></foreignObject>
    </svg>
    {spec.profile.narrated && spec.scenes.map((item) => {
      const voice = item.narration;
      if (voice.timing_basis !== 'measured' || !voice.file?.startsWith('public/')) {
        throw new Error(`Scene ${item.id} needs measured audio under public/ before rendering`);
      }
      const from = frameAt(item.start + voice.offset, fps);
      return <Sequence key={item.id} from={from} durationInFrames={frameAt(voice.duration_seconds, fps)} layout="none">
        <Audio src={staticFile(voice.file.slice('public/'.length))} />
      </Sequence>;
    })}
  </AbsoluteFill>;
}

function Root() {
  if (!Number.isInteger(spec.profile.fps) || spec.profile.renderer !== 'remotion') {
    throw new Error('This starter needs Remotion and an integer frame rate');
  }
  return <Composition id="Explainer" component={Explainer} defaultProps={{spec}}
    width={spec.profile.width} height={spec.profile.height} fps={spec.profile.fps}
    durationInFrames={compositionFrames(spec)} />;
}
registerRoot(Root);
