import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
  groupName: string;
  family?: string;
};

export const GroupLink: React.FC<Props> = ({ workspaceName, groupName, family, children }) => {
  const def =
    family === "groups"
      ? { href: "/[workspaceName]/groups/[groupName]", as: uri`/${workspaceName}/groups/${groupName}` }
      : { href: "/[workspaceName]/stats/[groupName]", as: uri`/${workspaceName}/stats/${groupName}` };
  return (
    <Link href={def.href} as={def.as}>
      {children != null ? children : <a>{groupName}</a>}
    </Link>
  );
};

export default GroupLink;
