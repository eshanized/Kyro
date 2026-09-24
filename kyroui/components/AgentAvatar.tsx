import React from "react";
import { Bot, Zap, Wrench, Globe, Terminal, Code, Cpu, Sparkles, Shield, User } from "lucide-react";

export interface AgentAvatarProps {
  avatar?: string;
  name?: string;
  className?: string;
  iconClassName?: string;
}

export const getAgentIconComponent = (avatar?: string, name?: string) => {
  const normAvatar = (avatar || "").trim();
  const normName = (name || "").toLowerCase().trim();

  // Explicit emoji / keyword mappings
  if (normAvatar === "\u26A1" || normAvatar === "zap" || normName.includes("apex")) {
    return Zap;
  }
  if (
    normAvatar === "\uD83D\uDEE0\uFE0F" ||
    normAvatar === "\uD83D\uDEE0" ||
    normAvatar === "wrench" ||
    normName.includes("forge")
  ) {
    return Wrench;
  }
  if (normAvatar === "\uD83C\uDF10" || normAvatar === "globe" || normName.includes("scout")) {
    return Globe;
  }
  if (normAvatar === "terminal" || normAvatar === "pty") {
    return Terminal;
  }
  if (normAvatar === "code") {
    return Code;
  }
  if (normAvatar === "cpu") {
    return Cpu;
  }
  if (normAvatar === "sparkles") {
    return Sparkles;
  }
  if (normAvatar === "shield") {
    return Shield;
  }
  if (normAvatar === "user") {
    return User;
  }

  // Default agent icon
  return Bot;
};

export const AgentAvatar: React.FC<AgentAvatarProps> = ({
  avatar = "bot",
  name = "",
  className = "w-7 h-7 rounded-lg bg-neutral-900 border border-neutral-800 flex items-center justify-center text-neutral-300",
  iconClassName = "w-4 h-4",
}) => {
  const IconComponent = getAgentIconComponent(avatar, name);

  return (
    <div className={`shrink-0 ${className}`} aria-label={name ? `@${name}` : "Agent"}>
      <IconComponent className={iconClassName} />
    </div>
  );
};

export default AgentAvatar;
