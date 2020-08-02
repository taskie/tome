import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
};

export const WorkspaceLink: React.FC<Props> = ({ workspaceName, children }) => {
  const def = { href: "/[workspaceName]", as: uri`/${workspaceName}` };
  return (
    <Link href={def.href} as={def.as}>
      {children != null ? children : <a>{workspaceName}</a>}
    </Link>
  );
};

export default WorkspaceLink;
