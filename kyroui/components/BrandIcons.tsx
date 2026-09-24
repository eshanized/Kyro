import React from "react";
import { Plug, Server, Terminal, Network } from "lucide-react";

export interface IconBaseProps extends React.SVGProps<SVGSVGElement> {
  className?: string;
  size?: number | string;
}

export const GithubIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    className={`inline-block shrink-0 ${className}`}
    role="img"
    aria-label="GitHub"
    {...props}
  >
    <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
  </svg>
);

export const GitlabIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#FC6D26" }}
    role="img"
    aria-label="GitLab"
    {...props}
  >
    <path d="m23.6004 9.5927-.0337-.0862L20.3.9814a.851.851 0 0 0-.3362-.405.8748.8748 0 0 0-.9997.0539.8748.8748 0 0 0-.29.4399l-2.2055 6.748H7.5375l-2.2057-6.748a.8573.8573 0 0 0-.29-.4412.8748.8748 0 0 0-.9997-.0537.8585.8585 0 0 0-.3362.4049L.4332 9.5015l-.0325.0862a6.0657 6.0657 0 0 0 2.0119 7.0105l.0113.0087.03.0213 4.976 3.7264 2.462 1.8633 1.4995 1.1321a1.0085 1.0085 0 0 0 1.2197 0l1.4995-1.1321 2.4619-1.8633 5.006-3.7489.0125-.01a6.0682 6.0682 0 0 0 2.0094-7.003z" />
  </svg>
);

export const SlackIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    className={`inline-block shrink-0 ${className}`}
    role="img"
    aria-label="Slack"
    {...props}
  >
    <path
      fill="#36C5F0"
      d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313z"
    />
    <path
      fill="#2EB67D"
      d="M8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312z"
    />
    <path
      fill="#ECB22E"
      d="M18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312z"
    />
    <path
      fill="#E01E5A"
      d="M15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z"
    />
  </svg>
);

export const LinearIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#5E6AD2" }}
    role="img"
    aria-label="Linear"
    {...props}
  >
    <path d="M2.886 4.18A11.982 11.982 0 0 1 11.99 0C18.624 0 24 5.376 24 12.009c0 3.64-1.62 6.903-4.18 9.105L2.887 4.18ZM1.817 5.626l16.556 16.556c-.524.33-1.075.62-1.65.866L.951 7.277c.247-.575.537-1.126.866-1.65ZM.322 9.163l14.515 14.515c-.71.172-1.443.282-2.195.322L0 11.358a12 12 0 0 1 .322-2.195Zm-.17 4.862 9.823 9.824a12.02 12.02 0 0 1-9.824-9.824Z" />
  </svg>
);

export const GoogleDriveIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    className={`inline-block shrink-0 ${className}`}
    role="img"
    aria-label="Google Drive"
    {...props}
  >
    <path
      fill="#4285F4"
      d="M12.01 1.485c-2.082 0-3.754.02-3.743.047.01.02 1.708 3.001 3.774 6.62l3.76 6.574h3.76c2.081 0 3.753-.02 3.742-.047-.005-.02-1.708-3.001-3.775-6.62l-3.76-6.574z"
    />
    <path
      fill="#0F9D58"
      d="M7.25 3.214a789.828 789.861 0 0 0-3.63 6.319L0 15.868l1.89 3.298 1.885 3.297 3.62-6.335 3.618-6.33-1.88-3.287C8.1 4.704 7.255 3.22 7.25 3.214z"
    />
    <path
      fill="#FFBA00"
      d="m9.509 15.867-.203.348c-.114.198-.96 1.672-1.88 3.287a423.93 423.948 0 0 1-1.698 2.97c-.01.026 3.24.042 7.222.042h7.244l1.796-3.157c.992-1.734 1.85-3.23 1.906-3.323l.104-.167h-7.249z"
    />
  </svg>
);

export const GoogleCalendarIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#4285F4" }}
    role="img"
    aria-label="Google Calendar"
    {...props}
  >
    <path
      fill="#4285F4"
      d="M18.316 5.684H24v12.632h-5.684V5.684zM5.684 24h12.632v-5.684H5.684V24zM18.316 5.684V0H1.895A1.894 1.894 0 0 0 0 1.895v16.421h5.684V5.684h12.632zm-7.207 6.25v-.065c.272-.144.5-.349.687-.617s.279-.595.279-.982c0-.379-.099-.72-.3-1.025a2.05 2.05 0 0 0-.832-.714 2.703 2.703 0 0 0-1.197-.257c-.6 0-1.094.156-1.481.467-.386.311-.65.671-.793 1.078l1.085.452c.086-.249.224-.461.413-.633.189-.172.445-.257.767-.257.33 0 .602.088.816.264a.86.86 0 0 1 .322.703c0 .33-.12.589-.36.778-.24.19-.535.284-.886.284h-.567v1.085h.633c.407 0 .748.109 1.02.327.272.218.407.499.407.843 0 .336-.129.614-.387.832s-.565.327-.924.327c-.351 0-.651-.103-.897-.311-.248-.208-.422-.502-.521-.881l-1.096.452c.178.616.505 1.082.977 1.401.472.319.984.478 1.538.477a2.84 2.84 0 0 0 1.293-.291c.382-.193.684-.458.902-.794.218-.336.327-.72.327-1.149 0-.429-.115-.797-.344-1.105a2.067 2.067 0 0 0-.881-.689zm2.093-1.931l.602.913L15 10.045v5.744h1.187V8.446h-.827l-2.158 1.557zM22.105 0h-3.289v5.184H24V1.895A1.894 1.894 0 0 0 22.105 0zm-3.289 23.5l4.684-4.684h-4.684V23.5zM0 22.105C0 23.152.848 24 1.895 24h3.289v-5.184H0v3.289z"
    />
  </svg>
);

export const GmailIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#EA4335" }}
    role="img"
    aria-label="Gmail"
    {...props}
  >
    <path
      fill="currentColor"
      d="M24 5.457v13.909c0 .904-.732 1.636-1.636 1.636h-3.819V11.73L12 16.64l-6.545-4.91v9.273H1.636A1.636 1.636 0 0 1 0 19.366V5.457c0-2.023 2.309-3.178 3.927-1.964L5.455 4.64 12 9.548l6.545-4.91 1.528-1.145C21.69 2.28 24 3.434 24 5.457z"
    />
  </svg>
);

export const TelegramIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#26A5E4" }}
    role="img"
    aria-label="Telegram"
    {...props}
  >
    <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.48.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z" />
  </svg>
);

export const NvidiaIcon: React.FC<IconBaseProps> = ({ className = "w-5 h-5", size, ...props }) => (
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="currentColor"
    className={`inline-block shrink-0 ${className}`}
    style={{ color: "#76B900" }}
    role="img"
    aria-label="NVIDIA"
    {...props}
  >
    <path d="M8.948 8.798v-1.43a6.7 6.7 0 0 1 .424-.018c3.922-.124 6.493 3.374 6.493 3.374s-2.774 3.851-5.75 3.851c-.398 0-.787-.062-1.158-.185v-4.346c1.528.185 1.837.857 2.747 2.385l2.04-1.714s-1.492-1.952-4-1.952a6.016 6.016 0 0 0-.796.035m0-4.735v2.138l.424-.027c5.45-.185 9.01 4.47 9.01 4.47s-4.08 4.964-8.33 4.964c-.37 0-.733-.035-1.095-.097v1.325c.3.035.61.062.91.062 3.957 0 6.82-2.023 9.593-4.408.459.371 2.34 1.263 2.73 1.652-2.633 2.208-8.772 3.984-12.253 3.984-.335 0-.653-.018-.971-.053v1.864H24V4.063zm0 10.326v1.131c-3.657-.654-4.673-4.46-4.673-4.46s1.758-1.944 4.673-2.262v1.237H8.94c-1.528-.186-2.73 1.245-2.73 1.245s.68 2.412 2.739 3.11M2.456 10.9s2.164-3.197 6.5-3.533V6.201C4.153 6.59 0 10.653 0 10.653s2.35 6.802 8.948 7.42v-1.237c-4.84-.6-6.492-5.936-6.492-5.936z" />
  </svg>
);

export interface ServiceIconProps {
  id?: string;
  name?: string;
  className?: string;
  size?: number | string;
}

export const ServiceIcon: React.FC<ServiceIconProps> = ({ id = "", name = "", className = "w-5 h-5", size }) => {
  const norm = (id || name).toLowerCase().replace(/[^a-z0-9]/g, "");

  if (norm.includes("github")) {
    return <GithubIcon className={className} size={size} />;
  }
  if (norm.includes("gitlab")) {
    return <GitlabIcon className={className} size={size} />;
  }
  if (norm.includes("slack")) {
    return <SlackIcon className={className} size={size} />;
  }
  if (norm.includes("linear")) {
    return <LinearIcon className={className} size={size} />;
  }
  if (norm.includes("googledrive") || norm.includes("gdrive") || norm.includes("drive")) {
    return <GoogleDriveIcon className={className} size={size} />;
  }
  if (norm.includes("googlecalendar") || norm.includes("gcalendar") || norm.includes("calendar")) {
    return <GoogleCalendarIcon className={className} size={size} />;
  }
  if (norm.includes("gmail") || norm.includes("email")) {
    return <GmailIcon className={className} size={size} />;
  }
  if (norm.includes("telegram")) {
    return <TelegramIcon className={className} size={size} />;
  }
  if (norm.includes("nvidia") || norm.includes("nemotron")) {
    return <NvidiaIcon className={className} size={size} />;
  }
  if (norm.includes("remotemcp") || norm.includes("network") || norm.includes("web") || norm.includes("http")) {
    return <Network className={className} />;
  }
  if (norm.includes("localmcp") || norm.includes("terminal") || norm.includes("stdio")) {
    return <Terminal className={className} />;
  }
  if (norm.includes("mcp") || norm.includes("tool")) {
    return <Plug className={className} />;
  }

  return <Server className={className} />;
};

export default ServiceIcon;
