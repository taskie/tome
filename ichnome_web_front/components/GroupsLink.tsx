import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
};

export const GroupsLink: React.FC<Props> = ({ workspaceName, children }) => {
  const def = { href: "/[workspaceName]/groups", as: uri`/${workspaceName}/groups` };
  return (
    <Link href={def.href} as={def.as}>
      {children != null ? children : <a>{workspaceName}</a>}
    </Link>
  );
};

export default GroupsLink;
