export const smoothTransition = {
  duration: 0.22,
  ease: [0.22, 1, 0.36, 1] as const,
};

export const microTransition = {
  duration: 0.16,
  ease: [0.22, 1, 0.36, 1] as const,
};

export const statusPulseTransition = {
  duration: 1.6,
  repeat: Infinity,
  ease: "easeInOut" as const,
};

export const panelVariants = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -6 },
};

export const itemVariants = {
  initial: { opacity: 0, y: 4 },
  animate: { opacity: 1, y: 0 },
};
